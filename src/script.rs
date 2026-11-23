// SPDX-License-Identifier: EUPL-1.2
//! A script: a file of SQL statements, read in order as SQL Server's tools, Snowflake and
//! DuckDB write them, and what its reader finds in it.

use std::collections::HashSet;
use std::ops::Range;

use polyglot_sql::expressions::{ExecuteStatement, Expression, Literal};
use polyglot_sql::{Token, TokenType, generate, parse_one};

use crate::dialect::Dialect;
use crate::relation::{Defaults, Name, RelationName};
use crate::write::{Place, Write};

/// The first words of T-SQL's statements, before which a window may end; not `WITH`, which
/// also starts a table hint.
const CUTS: [&str; 26] = [
    "ALTER",
    "BEGIN",
    "CREATE",
    "DECLARE",
    "DELETE",
    "DENY",
    "DROP",
    "ELSE",
    "END",
    "EXEC",
    "EXECUTE",
    "GRANT",
    "IF",
    "INSERT",
    "MERGE",
    "PRINT",
    "RAISERROR",
    "RETURN",
    "REVOKE",
    "SELECT",
    "SET",
    "THROW",
    "TRUNCATE",
    "UPDATE",
    "USE",
    "WHILE",
];

/// What a T-SQL batch creates or alters when it is one statement: a view or a routine.
const ROUTINES: [TokenType; 4] = [TokenType::View, TokenType::Procedure, TokenType::Function, TokenType::Trigger];

/// The first word of a statement that makes it a write, with the words that it needs at
/// depth 0, before its first `AS` and anywhere.
const WRITES: [(TokenType, &[TokenType], &[TokenType]); 9] = [
    (TokenType::Create, &[TokenType::View], &[]),
    (TokenType::Create, &[TokenType::Table], &[TokenType::As]),
    (TokenType::Alter, &[TokenType::View], &[]),
    (TokenType::Insert, &[], &[]),
    (TokenType::Update, &[], &[]),
    (TokenType::Delete, &[], &[]),
    (TokenType::Merge, &[], &[]),
    (TokenType::With, &[], &[]),
    (TokenType::Select, &[], &[TokenType::Into]),
];

/// A write found in a script: its id, the place of its first token in its file; its
/// target, as its statement names it; and the relations that its pieces read, as they
/// name them, without repeats.
pub struct Scripted {
    pub id: String,
    pub target: RelationName,
    pub reads: Vec<RelationName>,
}

/// What a reader finds in a script besides its writes, with its place: a statement
/// refused, with polyglot's message, or text not read.
pub enum Finding {
    Refused { place: String, detail: String },
    NotRead { place: String, detail: String },
}

/// A file read as a script: its writes, in reading order, its findings, and the number of
/// statements skipped; its path, and the defaults as they stand.
pub struct Script {
    pub writes: Vec<Scripted>,
    pub findings: Vec<Finding>,
    pub skipped: usize,
    path: String,
    defaults: Defaults,
}

impl Script {
    /// The script that `bytes`, the file at `path`, hold under `dialect`, its names
    /// completed by `defaults` until a `USE` changes them. A byte order mark chooses UTF-8,
    /// UTF-16 LE or UTF-16 BE; none, UTF-8. The text, tokenized once, is split at each `;`
    /// at depth 0 and, in T-SQL, into batches at each line of `GO` and each SQLCMD line,
    /// a batch that creates or alters a view or a routine being one statement; the rest of
    /// T-SQL's into windows, each ending before the next cut and growing over it while its
    /// parse fails at its last token. A file that does not decode or tokenize is not read.
    pub fn read(path: &str, bytes: &[u8], dialect: Dialect, defaults: Defaults) -> Script {
        let mut script =
            Script { writes: Vec::new(), findings: Vec::new(), skipped: 0, path: path.to_string(), defaults };
        let utf16 = |units: &[u8], unit: fn([u8; 2]) -> u16| {
            let (pairs, rest) = units.as_chunks::<2>();
            let text = char::decode_utf16(pairs.iter().map(|pair| unit(*pair))).collect::<Result<String, _>>();
            text.ok().filter(|_| rest.is_empty())
        };
        let text = match bytes {
            [0xFF, 0xFE, units @ ..] => utf16(units, u16::from_le_bytes),
            [0xFE, 0xFF, units @ ..] => utf16(units, u16::from_be_bytes),
            _ => String::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes).to_vec()).ok(),
        };
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
                depth +=
                    i32::from(token.token_type == TokenType::LParen) - i32::from(token.token_type == TokenType::RParen);
                let word = !matches!(
                    token.token_type,
                    TokenType::QuotedIdentifier | TokenType::String | TokenType::NationalString
                );
                let cut = tsql && first(at) && word && CUTS.contains(&token.text.to_uppercase().as_str());
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
                match parse_one(&window, dialect.polyglot()) {
                    // A window that fails at its last token ran out of text: it grows.
                    Err(error) if !hard && error.start().is_some_and(|at| start + at >= tokens[to - 1].span.start) => {}
                    parsed => {
                        script.statement(parsed, &window, &tokens[from..to], Place::at(&starts, start), dialect);
                        from = to + usize::from(hard);
                    }
                }
            }
        }
        script
    }

    /// Reads `parsed`, the parse of `text`, a statement of the script at `place` whose
    /// tokens are `tokens`: a write; a `USE`, which sets the defaults; SSMS's existence
    /// check, `sp_executesql` of a national string as `@statement` that parses, read as that
    /// statement; any other dynamic SQL, not read; any other statement, skipped. One that
    /// does not parse is refused, reported when its first words make it a write.
    fn statement(
        &mut self,
        parsed: polyglot_sql::Result<Expression>,
        text: &str,
        tokens: &[Token],
        place: Place,
        dialect: Dialect,
    ) {
        let id = format!("{}:{place}", self.path);
        let sql: Vec<char> = text.chars().collect();
        let tree = match parsed {
            Ok(tree) => tree,
            Err(error) => {
                let mut depth = 0;
                let words = tokens.iter().filter(|token| {
                    depth += i32::from(token.token_type == TokenType::LParen)
                        - i32::from(token.token_type == TokenType::RParen);
                    depth == 0
                });
                let words: Vec<TokenType> = words.map(|token| token.token_type).collect();
                let head: Vec<TokenType> = words.iter().copied().take_while(|word| *word != TokenType::As).collect();
                let write = |(first, before, anywhere): &(TokenType, &[TokenType], &[TokenType])| {
                    let needs =
                        |needed: &[TokenType], among: &[TokenType]| needed.iter().all(|word| among.contains(word));
                    words.first() == Some(first) && needs(before, &head) && needs(anywhere, &words)
                };
                if WRITES.iter().any(write) {
                    self.findings.push(Finding::Refused { place: id, detail: error.to_string() });
                } else {
                    self.skipped += 1;
                }
                return;
            }
        };
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
            self.writes.push(Scripted { id, target, reads });
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
        let run = |execute: &ExecuteStatement| {
            let given =
                execute.parameters.iter().find(|parameter| parameter.name.eq_ignore_ascii_case("@statement"))?;
            let Expression::Literal(literal) = &given.value else { return None };
            let Literal::NationalString(statement) = literal.as_ref() else { return None };
            Some((parse_one(statement, dialect.polyglot()).ok()?, statement.clone()))
        };
        match &tree {
            Expression::Use(used) => {
                let base = tokens.first().map_or(0, |token| token.span.start);
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
                Some((statement, text)) => self.statement(Ok(statement), &text, &[], place, dialect),
                None => self.findings.push(Finding::NotRead { place: id, detail: "dynamic SQL".to_string() }),
            },
            _ => self.skipped += 1,
        }
    }
}
