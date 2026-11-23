// SPDX-License-Identifier: EUPL-1.2
//! A script: a file of SQL statements, read in order as SQL Server's tools, Snowflake and
//! DuckDB write them, and what its reader finds in it.

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use polyglot_sql::expressions::{ExecuteStatement, Expression, Literal, TableConstraint};
use polyglot_sql::{Token, TokenType, generate, parse_one};
use serde::Serialize;

use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::relation::{Defaults, Name, RelationName};
use crate::write::{Place, Write, nesting};

/// The first words of T-SQL's statements, before which a window may end; not `WITH`, which
/// also starts a table hint.
const CUTS: &str = "ALTER BACKUP BEGIN BREAK BULK CHECKPOINT CLOSE COMMIT CONTINUE CREATE DBCC DEALLOCATE \
    DECLARE DELETE DENY DISABLE DROP ELSE ENABLE END EXEC EXECUTE FETCH GOTO GRANT IF INSERT KILL MERGE OPEN PRINT \
    RAISERROR READTEXT RECONFIGURE RESTORE RETURN REVERT REVOKE ROLLBACK SAVE SELECT SET SETUSER THROW TRUNCATE \
    UPDATE UPDATETEXT USE WAITFOR WHILE WRITETEXT";

/// polyglot-sql's message for the token after a complete statement, once its parse is done:
/// `Parser::parse` raises it there alone.
const BOUNDARY: &str = "Invalid expression / Unexpected token";

/// What a T-SQL batch creates or alters when it is one statement: a view or a routine.
const ROUTINES: [TokenType; 4] = [TokenType::View, TokenType::Procedure, TokenType::Function, TokenType::Trigger];

/// The first word of a statement that makes it a write or a declaration, with the words that
/// it needs at depth 0, before its first `AS` and anywhere.
const WRITES: [(TokenType, &[TokenType], &[TokenType]); 9] = [
    (TokenType::Create, &[TokenType::View], &[]),
    (TokenType::Create, &[TokenType::Table], &[]),
    (TokenType::Alter, &[TokenType::View], &[]),
    (TokenType::Insert, &[], &[]),
    (TokenType::Update, &[], &[]),
    (TokenType::Delete, &[], &[]),
    (TokenType::Merge, &[], &[]),
    (TokenType::With, &[], &[]),
    (TokenType::Select, &[], &[TokenType::Into]),
];

/// A write found in a script: its id, the place of its first token in its file; its
/// target, as its statement names it; whether it replaces its target; the relations that
/// its pieces read, as they name them, without repeats; for its answer, its statement's text
/// and the defaults in force there; and its place, with the words and the place of each of
/// its clauses, as [`Write::clauses`] finds them.
#[derive(Serialize)]
pub struct Scripted {
    pub id: String,
    pub target: RelationName,
    #[serde(skip)]
    pub replaces: bool,
    pub reads: Vec<RelationName>,
    #[serde(skip)]
    pub text: String,
    #[serde(skip)]
    pub defaults: Vec<Name>,
    #[serde(skip)]
    pub place: Place,
    #[serde(skip)]
    pub clauses: Vec<(String, Place)>,
}

/// What the reading of scripts finds besides their writes: a statement refused, with
/// polyglot's message or the catalog's rule that it breaks, text not read, or a relation
/// declared again, each with its place; once the writes are ordered, a cycle of writes,
/// with the relations that they write, or a relation replaced twice.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Finding {
    Refused { place: String, detail: String },
    NotRead { place: String, detail: String },
    DeclaredTwice { place: String, relation: RelationName },
    Cycle { writes: Vec<String>, relations: Vec<RelationName> },
    ReplacedTwice { relation: RelationName, writes: Vec<String> },
}

impl fmt::Display for Finding {
    /// Its kind and a colon, then its place and its detail on one line, its place and its
    /// relation, its writes and their relations, or its relation and its writes.
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let line = |detail: &str| detail.split_whitespace().collect::<Vec<_>>().join(" ");
        let names = |relations: &[RelationName]| relations.iter().map(ToString::to_string).collect::<Vec<_>>();
        match self {
            Finding::Refused { place, detail } => write!(formatter, "refused: {place} {}", line(detail)),
            Finding::NotRead { place, detail } => write!(formatter, "not_read: {place} {}", line(detail)),
            Finding::DeclaredTwice { place, relation } => write!(formatter, "declared_twice: {place} {relation}"),
            Finding::Cycle { writes, relations } => {
                write!(formatter, "cycle: {} ({})", writes.join(", "), names(relations).join(", "))
            }
            Finding::ReplacedTwice { relation, writes } => {
                write!(formatter, "replaced_twice: {relation} ({})", writes.join(", "))
            }
        }
    }
}

/// A file read as a script: its writes, in reading order, its findings, and the number of
/// statements skipped; its path, the defaults as they stand, and the catalog as its
/// declarations leave it.
pub struct Script {
    pub writes: Vec<Scripted>,
    pub findings: Vec<Finding>,
    pub skipped: usize,
    path: String,
    defaults: Defaults,
    pub catalog: Catalog,
}

impl Script {
    /// The script that `bytes`, the file at `path`, hold under `dialect`, its names
    /// completed by `defaults` until a `USE` changes them, its declarations added to
    /// `catalog`, as the scripts before it leave it. A byte order mark chooses UTF-8,
    /// UTF-16 LE or UTF-16 BE; none, UTF-8. The text, tokenized once, is split at each `;`
    /// at depth 0 and, in T-SQL, into batches at each line of `GO` and each SQLCMD line,
    /// a batch that creates or alters a view or a routine being one statement; the rest of
    /// T-SQL's into windows, each ending before the next cut and growing over it while its
    /// parse fails at its last token, or ending where polyglot-sql stops after a complete
    /// statement at a statement's first word; each parsed as [`parse`] reads it. A file that
    /// does not decode or tokenize is not read.
    pub fn read(path: &str, bytes: &[u8], dialect: Dialect, defaults: Defaults, catalog: Catalog) -> Script {
        let (writes, findings, path) = (Vec::new(), Vec::new(), path.to_string());
        let mut script = Script { writes, findings, skipped: 0, path, defaults, catalog };
        // UTF-16 after its byte order mark, which gives its order; else UTF-8, after its own or none.
        let utf16 = |mark: [u8; 2], unit: fn([u8; 2]) -> u16| {
            let (pairs, rest) = bytes.strip_prefix(&mark)?.as_chunks::<2>();
            let text = char::decode_utf16(pairs.iter().map(|pair| unit(*pair))).collect::<Result<String, _>>();
            Some(text.ok().filter(|_| rest.is_empty()))
        };
        let utf8 = || String::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes).to_vec()).ok();
        let text = (utf16([0xFF, 0xFE], u16::from_le_bytes))
            .or_else(|| utf16([0xFE, 0xFF], u16::from_be_bytes))
            .unwrap_or_else(utf8);
        let text = text.ok_or_else(|| "not UTF-8, nor UTF-16 after a byte order mark".to_string());
        let tokenize = |text: String| {
            let tokens = polyglot_sql::Dialect::get(dialect.polyglot()).tokenize(&text);
            Ok((tokens.map_err(|error| error.to_string())?, text))
        };
        let (tokens, text) = match text.and_then(tokenize) {
            Ok(read) => read,
            Err(detail) => {
                script.findings.push(Finding::NotRead { place: script.path.clone(), detail });
                return script;
            }
        };
        let chars: Vec<char> = text.chars().collect();
        // Where each line starts: a `\n` ends one, so a `\r` is its last column.
        let starts: Vec<usize> = std::iter::once(0)
            .chain(chars.iter().enumerate().filter(|(_, char)| **char == '\n').map(|(at, _)| at + 1))
            .collect();
        let first = |at: usize| at == 0 || chars[tokens[at - 1].span.end..tokens[at].span.start].contains(&'\n');
        let tsql = dialect == Dialect::Tsql;
        // Whether `token` is one of T-SQL's first words of statements.
        let opens = |token: &Token| {
            let word = !matches!(
                token.token_type,
                TokenType::QuotedIdentifier | TokenType::String | TokenType::NationalString
            );
            tsql && word && CUTS.split(' ').any(|cut| token.text.eq_ignore_ascii_case(cut))
        };
        let breaks: Vec<usize> = (0..tokens.len()).filter(|&at| first(at)).chain([tokens.len()]).collect();
        let lines: Vec<Range<usize>> = breaks.windows(2).map(|pair| pair[0]..pair[1]).collect();
        // T-SQL's separators of batches, which no statement holds: a line of `GO`, any case,
        // with an optional count, and a SQLCMD line, whose first token is `:`.
        let separator = |line: &Range<usize>| {
            let go = |token: &Token| token.token_type == TokenType::Var && token.text.eq_ignore_ascii_case("go");
            tsql && match &tokens[line.clone()] {
                [colon, ..] if colon.token_type == TokenType::Colon => true,
                [word] => go(word),
                [word, count] => go(word) && count.token_type == TokenType::Number,
                _ => false,
            }
        };
        let batches = lines.split(separator).filter_map(|batch| {
            let batch = batch.first()?.start..batch.last()?.end;
            let kinds: Vec<TokenType> = tokens[batch.clone()].iter().take(4).map(|token| token.token_type).collect();
            let whole = tsql
                && matches!(kinds.as_slice(),
                    [TokenType::Create, TokenType::Or, TokenType::Alter, kind, ..]
                    | [TokenType::Create | TokenType::Alter, kind, ..] if ROUTINES.contains(kind));
            Some((batch, whole))
        });
        for (batch, whole) in batches {
            let mut depth = 0;
            // Where a statement of the batch may end: for good at a `;` at depth 0, which no
            // statement holds; before a cut, a token first on its line at depth 0 among
            // T-SQL's first words; neither in a batch that is one statement.
            let ends = batch.clone().filter_map(|at| {
                let token = &tokens[at];
                depth += nesting(token);
                let cut = first(at) && opens(token);
                let semicolon = token.token_type == TokenType::Semicolon;
                (!whole && depth == 0 && (semicolon || cut)).then_some((at, semicolon))
            });
            let ends: Vec<(usize, bool)> = ends.chain([(batch.end, true)]).collect();
            let (mut from, mut next) = (batch.start, 0);
            while from < batch.end {
                let (to, hard) = ends[next];
                next += 1;
                if to == from {
                    from += usize::from(hard);
                    continue;
                }
                let start = tokens[from].span.start;
                let window: String = chars[start..tokens[to - 1].span.end].iter().collect();
                let parsed = parse(&window, dialect);
                // polyglot raises its unexpected token after a complete statement: at a statement's
                // first word, even within a line, the window ends there.
                let stop = parsed.as_ref().err().and_then(|error| match error {
                    polyglot_sql::Error::Parse { message, start: at, .. } if message == BOUNDARY => Some(start + at),
                    _ => None,
                });
                let cut = stop.map(|stop| tokens.partition_point(|token| token.span.start < stop));
                let cut = cut.filter(|&cut| from < cut && cut < to && opens(&tokens[cut]));
                // polyglot's message, at the place in the file where it stops, not in the window.
                let detail = |error| match error {
                    polyglot_sql::Error::Parse { message, start: at, .. } => {
                        format!("{message}, at {}", Place::at(&starts, start + at))
                    }
                    error => error.to_string(),
                };
                // The place in the file of an offset in the statement.
                let at = |offset: usize| Place::at(&starts, start + offset);
                match (cut, parsed) {
                    (Some(cut), _) => {
                        let head: String = chars[start..tokens[cut - 1].span.end].iter().collect();
                        let parsed = parse(&head, dialect).map_err(detail);
                        script.statement(parsed, &head, &tokens[from..cut], &at, dialect);
                        (from, next) = (cut, next - 1);
                    }
                    // A window that fails at its last token ran out of text: it grows.
                    (None, Err(error))
                        if !hard && error.start().is_some_and(|at| start + at >= tokens[to - 1].span.start) => {}
                    (None, parsed) => {
                        script.statement(parsed.map_err(detail), &window, &tokens[from..to], &at, dialect);
                        from = to + usize::from(hard);
                    }
                }
            }
        }
        script
    }

    /// Reads `parsed`, the parse of `text`, a statement of the script whose tokens are
    /// `tokens` and whose offsets are at the places that `at` gives, as [`Script::parsed`]
    /// reads its tree. One that does not parse is refused, reported when its first words make
    /// it a write or a declaration.
    fn statement(
        &mut self,
        parsed: Result<Expression, String>,
        text: &str,
        tokens: &[Token],
        at: &dyn Fn(usize) -> Place,
        dialect: Dialect,
    ) {
        let place = at(0);
        let error = match parsed {
            Ok(tree) => return self.parsed(tree, text, tokens, place, at, dialect),
            Err(error) => error,
        };
        let mut depth = 0;
        let words = tokens.iter().filter(|token| {
            depth += nesting(token);
            depth == 0
        });
        let words: Vec<TokenType> = words.map(|token| token.token_type).collect();
        let head: Vec<TokenType> = words.iter().copied().take_while(|word| *word != TokenType::As).collect();
        let write = |(first, before, anywhere): &(TokenType, &[TokenType], &[TokenType])| {
            let needs = |needed: &[TokenType], among: &[TokenType]| needed.iter().all(|word| among.contains(word));
            words.first() == Some(first) && needs(before, &head) && needs(anywhere, &words)
        };
        if WRITES.iter().any(write) {
            self.findings.push(Finding::Refused { place: format!("{}:{place}", self.path), detail: error });
        } else {
            self.skipped += 1;
        }
    }

    /// Reads `tree`, the tree of `text`, a statement of the script at `place` whose tokens
    /// are `tokens` and whose offsets are at the places that `at` gives: a write, with the
    /// places of its clauses; a declaration, a `CREATE TABLE` with a column list and no
    /// query, which goes to the catalog, refused when it breaks the catalog's rule, and
    /// reported when it declares its relation again; a `USE`, which sets the defaults;
    /// SSMS's existence check, `sp_executesql` of a national string as `@statement` that
    /// parses, read as that statement, at the places of its characters between the string's
    /// quotes, each doubled quote once; any other dynamic SQL, not read; any other statement,
    /// skipped.
    fn parsed(
        &mut self,
        tree: Expression,
        text: &str,
        tokens: &[Token],
        place: Place,
        at: &dyn Fn(usize) -> Place,
        dialect: Dialect,
    ) {
        let id = format!("{}:{place}", self.path);
        let sql: Vec<char> = text.chars().collect();
        let defaults = self.defaults.read(dialect).expect("the defaults read, as they did when given");
        let read = Write::read(&tree, text, dialect, 0).ok();
        let target = read.as_ref().and_then(|read| RelationName::of(read.table?, &sql, dialect, &defaults));
        if let (Some(read), Some(target)) = (read, target) {
            // Each piece's text, as answers read it: polyglot's scopes take no table from the
            // `Alias` that a `MERGE`'s tree holds for its target and its source.
            let pieces = read.pieces.iter().map(|(piece, _)| {
                piece.as_ref().map_or_else(|| Ok(text.to_string()), |piece| generate(piece, dialect.polyglot()))
            });
            let reads = pieces.flatten().flat_map(|piece| {
                let query = Write::query(&piece, dialect).ok();
                query.map(|query| Write::relations(&piece, &query, dialect, &defaults)).unwrap_or_default()
            });
            let mut seen = HashSet::new();
            let reads = reads.filter(|relation| seen.insert(relation.clone())).collect();
            let clauses =
                Write::clauses(text, dialect).into_iter().map(|(words, offset)| (words, at(offset))).collect();
            let text = text.to_string();
            self.writes.push(Scripted { id, target, replaces: read.replaces, reads, text, defaults, place, clauses });
            return;
        }
        // Dynamic SQL: `EXEC` of a string or a variable, or of `sp_executesql`, bare or under
        // `dbo`, which SSMS's existence check calls.
        let dbo: Vec<Name> = Name::read("dbo", dialect).into_iter().collect();
        let executesql = RelationName::read("dbo.sp_executesql", dialect, &dbo).ok();
        let dynamic = |execute: &ExecuteStatement| match &execute.this {
            Expression::Paren(_) => true,
            Expression::Table(table) => RelationName::of(table, &sql, dialect, &dbo) == executesql,
            _ => false,
        };
        let base = tokens.first().map_or(0, |token| token.span.start);
        let run = |execute: &ExecuteStatement| {
            let given =
                execute.parameters.iter().find(|parameter| parameter.name.eq_ignore_ascii_case("@statement"))?;
            let Expression::Literal(literal) = &given.value else { return None };
            let Literal::NationalString(statement) = literal.as_ref() else { return None };
            // The offset in this statement of each character of that one: past `N'`, and past
            // each doubled quote's first.
            let mut string = tokens.iter().skip_while(|token| !token.text.eq_ignore_ascii_case("@statement"));
            let string = string.find(|token| token.token_type == TokenType::NationalString)?;
            let offsets = statement.chars().scan(string.span.start - base + 2, |next, char| {
                let offset = *next;
                *next += 1 + usize::from(char == '\'');
                Some(offset)
            });
            Some((parse(statement, dialect).ok()?, statement.clone(), offsets.collect::<Vec<_>>()))
        };
        // `LIKE` and `CLONE` give a table no list of columns.
        let like = |constraint: &TableConstraint| matches!(constraint, TableConstraint::Like { .. });
        match &tree {
            Expression::Use(used) => {
                let named = tokens.iter().skip(1);
                let named = named.filter(|token| {
                    !matches!(token.token_type, TokenType::Database | TokenType::Schema | TokenType::Dot)
                });
                let parts: Vec<String> =
                    named.map(|token| sql[token.span.start - base..token.span.end - base].iter().collect()).collect();
                match self.defaults.used(used.kind, &parts, dialect) {
                    Some(defaults) => self.defaults = defaults,
                    None => self.skipped += 1,
                }
            }
            Expression::Execute(execute) if dynamic(execute) => match run(execute) {
                Some((statement, text, offsets)) => {
                    self.parsed(statement, &text, &[], place, &|offset| at(offsets[offset]), dialect)
                }
                None => self.findings.push(Finding::NotRead { place: id, detail: "dynamic SQL".to_string() }),
            },
            Expression::CreateTable(create)
                if create.clone_source.is_none() && !create.constraints.iter().any(like) =>
            {
                let columns = create.columns.iter().map(|column| Name::of(&column.name, &sql, dialect)).collect();
                let relation = RelationName::of(&create.name, &sql, dialect, &defaults);
                let unread = || "a name that it declares does not read".to_string();
                let declared = (relation.zip(columns).ok_or_else(unread))
                    .and_then(|(relation, columns)| Ok(self.catalog.declare(&relation, columns)?.then_some(relation)));
                match declared {
                    Ok(again) => {
                        self.findings.extend(again.map(|relation| Finding::DeclaredTwice { place: id, relation }))
                    }
                    Err(detail) => self.findings.push(Finding::Refused { place: id, detail }),
                }
            }
            _ => self.skipped += 1,
        }
    }
}

/// The parse of `text`, a statement, under `dialect`. In T-SQL, a declaration, a
/// `CREATE TABLE` with no `AS` outside parentheses, is read with its storage clauses, which
/// never bear on lineage, replaced by spaces: in its column list, its first parentheses,
/// `ROWGUIDCOL`, `SPARSE` and `FILESTREAM` after a column's first token; after it, outside
/// parentheses, `TEXTIMAGE_ON` and `FILESTREAM_ON` with their filegroup, `ON` with its
/// filegroup but not a partition scheme, whose name a `(` follows, and `WITH` with its
/// options. Words in strings and quoted names are no clauses' words; comments, newlines and
/// every place are kept.
fn parse(text: &str, dialect: Dialect) -> Result<Expression, polyglot_sql::Error> {
    let tokens = polyglot_sql::Dialect::get(dialect.polyglot()).tokenize(text).unwrap_or_default();
    let quoted = [TokenType::QuotedIdentifier, TokenType::String, TokenType::NationalString];
    let word =
        |token: &Token| if quoted.contains(&token.token_type) { String::new() } else { token.text.to_uppercase() };
    // Each token's depth outside its own parentheses.
    let mut depth = 0;
    let depths: Vec<i32> = (tokens.iter())
        .map(|token| {
            let before = depth;
            depth += nesting(token);
            before.min(depth)
        })
        .collect();
    let first: Vec<TokenType> = tokens.iter().take(2).map(|token| token.token_type).collect();
    let declaration = dialect == Dialect::Tsql
        && first == [TokenType::Create, TokenType::Table]
        && !tokens.iter().zip(&depths).any(|(token, depth)| *depth == 0 && word(token) == "AS");
    let list =
        tokens.iter().zip(&depths).position(|(token, depth)| *depth == 0 && token.token_type == TokenType::RParen);
    let Some(list) = list.filter(|_| declaration) else { return parse_one(text, dialect.polyglot()) };
    let mut chars: Vec<char> = text.chars().collect();
    let opens = |token: &Token| [TokenType::LParen, TokenType::Comma].contains(&token.token_type);
    let mut blanked = false;
    for (at, token) in tokens.iter().enumerate() {
        let word = word(token);
        if at > list && depths[at] == 0 && ["ON", "TEXTIMAGE_ON", "FILESTREAM_ON", "WITH"].contains(&word.as_str()) {
            blanked = word != "ON" || tokens.get(at + 2).is_none_or(|next| next.token_type != TokenType::LParen);
        }
        let attribute = depths[at] > 0 && ["ROWGUIDCOL", "SPARSE", "FILESTREAM"].contains(&word.as_str());
        if (at < list && attribute && !opens(&tokens[at - 1])) || (at > list && blanked) {
            chars[token.span.start..token.span.end]
                .iter_mut()
                .filter(|char| **char != '\n')
                .for_each(|char| *char = ' ');
        }
    }
    parse_one(&chars.into_iter().collect::<String>(), dialect.polyglot())
}
