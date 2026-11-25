// SPDX-License-Identifier: EUPL-1.2
//! The commands, as the command line names them, and what they print.

use std::path::PathBuf;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{ArgAction, Args, Parser, Subcommand};
use clap_complete::Shell;
use polyglot_sql::TokenType;
use serde_json::json;
use tracing::field::Empty;

use crate::answer::Answer;
use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::edge::End;
use crate::graph::Graph;
use crate::lineage::Lineage;
use crate::relation::{Defaults, Name, RelationName, listing};
use crate::script::{Script, Scripted};

/// The command line: `--describe`, or a command; with neither, a request on standard input.
#[derive(Parser)]
#[command(version, about, args_conflicts_with_subcommands = true)]
#[command(after_help = "With no argument, a request of protocol 1 on standard input, unless it is a terminal.")]
pub struct Cli {
    /// Print what this build is, as JSON.
    #[arg(long, conflicts_with = "verbose")]
    pub describe: bool,
    /// Log on standard error: -v info, -vv debug, -vvv trace.
    #[arg(short, action = ArgAction::Count, global = true)]
    pub verbose: u8,
    #[command(subcommand)]
    pub verb: Option<Verb>,
}

/// The options that the commands share, and their paths.
#[derive(Args)]
pub struct Options {
    /// The dialect of the scripts.
    #[arg(long, value_parser = PossibleValuesParser::new(Dialect::built().into_iter().map(Dialect::name))
        .try_map(|name: String| Dialect::named(&name)))]
    dialect: Dialect,
    /// The database that completes a name of fewer than three parts.
    #[arg(long)]
    database: Option<String>,
    /// The schema that completes a name of fewer than three parts; for a database alone,
    /// the dialect's usual one.
    #[arg(long)]
    schema: Option<String>,
    /// Print JSON rather than text.
    #[arg(long)]
    json: bool,
    /// A file, or a directory whose files ending `.sql` are read, sorted by path.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

/// A command's argument, a column or a write, then its options and its paths.
#[derive(Args)]
pub struct Argument {
    #[arg(value_name = "RELATION.COLUMN")]
    argument: String,
    #[command(flatten)]
    options: Options,
}

/// What follows `static-sql-lineage`: a command, with its argument, its options and its
/// paths; `completions` and its shell; or `-`.
#[derive(Subcommand)]
pub enum Verb {
    /// The writes of the scripts, in the order to migrate them in.
    Graph(Options),
    /// A column's edges upstream, through the writes that fill its relation.
    Trace(Argument),
    /// A column's edges downstream, through the writes that read it.
    Uses(Argument),
    /// One write in brief, by its id as `graph` prints it.
    #[command(mut_args(|arg| if arg.get_id() == "argument" { arg.value_name("ID") } else { arg }))]
    Outline(Argument),
    /// A script that completes the command line in a shell.
    Completions {
        #[arg(value_parser = PossibleValuesParser::new(["bash", "zsh", "fish"]).try_map(|name: String| name.parse::<Shell>()))]
        shell: Shell,
    },
    /// A request of protocol 1 on standard input, read even from a terminal.
    #[command(name = "-")]
    Request,
}

impl Verb {
    /// The command's argument, empty for `graph`, and its options.
    fn parts(&self) -> (&str, &Options) {
        match self {
            Verb::Graph(options) => ("", options),
            Verb::Trace(given) | Verb::Uses(given) | Verb::Outline(given) => (&given.argument, &given.options),
            Verb::Completions { .. } | Verb::Request => unreachable!("main prints completions and reads a request"),
        }
    }
}

/// A command to run: its verb, as the command line gives it, then its dialect and its
/// defaults, as its options give them.
pub struct Command {
    verb: Verb,
    dialect: Dialect,
    defaults: Defaults,
}

impl Command {
    /// The command that `verb` gives: `--dialect`, a dialect of the build; `--database` and
    /// `--schema`, the defaults, a database without a schema taking the dialect's usual one.
    /// Refused for a default that is not one identifier.
    pub fn read(verb: Verb) -> Result<Command, String> {
        let (_, options) = verb.parts();
        let dialect = options.dialect;
        let defaults = Defaults::given(options.database.clone(), options.schema.clone(), dialect);
        defaults.read(dialect)?;
        Ok(Command { verb, dialect, defaults })
    }

    /// What the command prints, or why it cannot run: a path that does not read, or an
    /// argument that names no column or write of the scripts. Each path is read in the order
    /// given, a directory's files ending `.sql`, in any case, sorted by path, into the graph of
    /// their writes and the catalog of their declarations; then `graph` prints the graph,
    /// `trace` and `uses` the walk from the argument's column, upstream or downstream, over the
    /// lineage of the writes, and `outline` the argument's write in brief.
    #[tracing::instrument(level = "info", skip_all, fields(dialect = %self.dialect.name(), files = Empty, writes = Empty))]
    pub fn run(&self) -> Result<String, String> {
        let (argument, options) = self.verb.parts();
        let unread = |path: &PathBuf, error: std::io::Error| format!("{} does not read: {error}", path.display());
        let (mut scripts, mut catalog) = (Vec::new(), Catalog::default());
        for given in &options.paths {
            // Each path to read, with whether it is a directory: a link to one found under a
            // directory is not, so that a cyclic link ends.
            let (mut pending, mut files) = (vec![(given.clone(), given.is_dir())], Vec::new());
            while let Some((path, directory)) = pending.pop() {
                if directory {
                    let entries = std::fs::read_dir(&path).map_err(|error| unread(&path, error))?;
                    let entries =
                        entries.map(|entry| entry.and_then(|entry| Ok((entry.path(), entry.file_type()?.is_dir()))));
                    pending.extend(entries.collect::<std::io::Result<Vec<_>>>().map_err(|error| unread(&path, error))?);
                } else if path == *given
                    || path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
                {
                    files.push(path);
                }
            }
            files.sort();
            for file in files {
                let bytes = std::fs::read(&file).map_err(|error| unread(&file, error))?;
                let path = file.display().to_string();
                let mut script = Script::read(&path, &bytes, self.dialect, self.defaults.clone(), catalog);
                catalog = std::mem::take(&mut script.catalog);
                scripts.push(script);
            }
        }
        tracing::Span::current().record("files", scripts.len());
        let graph = Graph::of(scripts);
        tracing::Span::current().record("writes", graph.writes.len());
        match self.verb {
            Verb::Graph(_) => Ok(graph.printed(options.json)),
            Verb::Outline(_) => {
                let at = graph.writes.iter().position(|write| write.id == argument);
                let at = at.ok_or_else(|| format!("no script has the write {argument}"))?;
                let lineage = Lineage::of(&graph, catalog, self.dialect);
                Ok(self.outline(&graph.writes[at], &lineage.answers[at]))
            }
            Verb::Trace(_) | Verb::Uses(_) => {
                // The argument's last part is its column, the rest its relation.
                let tokens = self.dialect.tokens(argument).unwrap_or_default();
                let dot = tokens.iter().rposition(|token| token.token_type == TokenType::Dot);
                let dot = dot.ok_or_else(|| format!("{argument:?} is not <relation>.<column>"))?;
                let chars: Vec<char> = argument.chars().collect();
                let (relation, column) = (&chars[..tokens[dot].span.start], &chars[tokens[dot].span.end..]);
                let defaults = self.defaults.read(self.dialect)?;
                let relation = RelationName::read(&relation.iter().collect::<String>(), self.dialect, &defaults)?;
                let from = End { relation, column: Name::read(&column.iter().collect::<String>(), self.dialect)? };
                let lineage = Lineage::of(&graph, catalog, self.dialect);
                let walk = lineage.walk(&from, matches!(self.verb, Verb::Trace(_)));
                Ok(walk.ok_or_else(|| format!("no script knows the column {from}"))?.printed(options.json))
            }
            Verb::Completions { .. } | Verb::Request => unreachable!("main prints completions and reads a request"),
        }
    }

    /// `write`, whose answer is `answer`, in brief: as JSON, or one field per line, then its
    /// answer's edges, gaps and refusal, then the words and the place of each of its clauses.
    pub fn outline(&self, write: &Scripted, answer: &Answer) -> String {
        let mode = if write.replaces { "replace" } else { "contribute" };
        if self.verb.parts().1.json {
            let clauses =
                write.clauses.iter().map(|(words, place)| json!({"keyword": words, "place": place.to_string()}));
            let clauses: Vec<_> = clauses.collect();
            let (id, target, place) = (&write.id, &write.target, write.place.to_string());
            return json!({
                "id": id, "target": target, "mode": mode, "place": place, "clauses": clauses, "answer": answer,
            })
            .to_string();
        }
        let fields = [
            format!("id: {}", write.id),
            format!("target: {}", write.target),
            format!("mode: {mode}"),
            format!("place: {}", write.place),
            format!("columns: {}", answer.columns.as_ref().map_or_else(|| "-".to_string(), listing)),
        ];
        let edges = answer.edges.iter().map(|edge| format!("edge: {edge}"));
        let gaps = answer.gaps.iter().map(|gap| format!("gap: {gap}"));
        let refusal = answer.refusal.iter().map(|refusal| format!("refused: {refusal}"));
        let clauses = write.clauses.iter().map(|(words, place)| format!("clause: {words} {place}"));
        fields.into_iter().chain(edges).chain(gaps).chain(refusal).chain(clauses).collect::<Vec<_>>().join("\n")
    }
}
