// SPDX-License-Identifier: EUPL-1.2
//! The commands, as the command line names them, and what they print.

use std::ffi::OsString;
use std::path::PathBuf;

use polyglot_sql::TokenType;
use serde_json::json;

use crate::answer::Answer;
use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::edge::End;
use crate::graph::Graph;
use crate::lineage::Lineage;
use crate::relation::{Defaults, Name, RelationName};
use crate::script::{Script, Scripted};

/// A command's name, as the command line gives it.
#[derive(Clone, Copy, PartialEq)]
pub enum Verb {
    Graph,
    Trace,
    Uses,
    Outline,
}

impl Verb {
    /// The commands.
    pub const ALL: [Verb; 4] = [Verb::Graph, Verb::Trace, Verb::Uses, Verb::Outline];

    /// The name that the command line gives the command.
    pub fn name(self) -> &'static str {
        match self {
            Verb::Graph => "graph",
            Verb::Trace => "trace",
            Verb::Uses => "uses",
            Verb::Outline => "outline",
        }
    }
}

/// A command as the command line gives it: its verb, its dialect, its defaults, whether it
/// prints JSON, its argument, for all but `graph`, and its paths, each a file, or a directory
/// whose files ending `.sql` it reads.
pub struct Command {
    verb: Verb,
    dialect: Dialect,
    defaults: Defaults,
    json: bool,
    argument: Option<String>,
    paths: Vec<PathBuf>,
}

impl Command {
    /// The command `verb` that `arguments`, those after its name, give: `--dialect`, a
    /// dialect of the build; `--database` and `--schema`, the defaults, a database without
    /// a schema taking the dialect's usual one; `--json`; then, but for `graph`, its
    /// argument; and its paths, at least one. Refused for an option that it does not know,
    /// or one without its value.
    pub fn read(verb: Verb, arguments: &[OsString]) -> Result<Command, String> {
        let name = verb.name();
        let (mut dialect, mut database, mut schema, mut json, mut paths) = (None, None, None, false, Vec::new());
        let mut arguments = arguments.iter();
        while let Some(argument) = arguments.next() {
            let mut value = || {
                let value = arguments.next().and_then(|value| value.to_str()).map(str::to_string);
                value.ok_or_else(|| format!("{argument:?} needs a value"))
            };
            match argument.to_str() {
                Some("--dialect") => dialect = Some(value()?),
                Some("--database") => database = Some(value()?),
                Some("--schema") => schema = Some(value()?),
                Some("--json") => json = true,
                Some(option) if option.starts_with("--") => return Err(format!("{name} has no option {option}")),
                _ => paths.push(PathBuf::from(argument)),
            }
        }
        let given = dialect.ok_or_else(|| format!("{name} needs --dialect"))?;
        let dialect = Dialect::named(&given)?;
        let defaults = Defaults::given(database, schema, dialect);
        defaults.read(dialect)?;
        let graph = verb == Verb::Graph;
        if paths.len() < 2 - usize::from(graph) {
            return Err(format!("{name} needs {}", if graph { "a path" } else { "its argument and a path" }));
        }
        let argument = (!graph).then(|| paths.remove(0).to_string_lossy().into_owned());
        Ok(Command { verb, dialect, defaults, json, argument, paths })
    }

    /// What the command prints, or why it cannot run: a path that does not read, or an
    /// argument that names no column or write of the scripts. Each path is read in the order
    /// given, a directory's files ending `.sql`, in any case, sorted by path, into the graph of
    /// their writes and the catalog of their declarations; then `graph` prints the graph,
    /// `trace` and `uses` the walk from the argument's column, upstream or downstream, over the
    /// lineage of the writes, and `outline` the argument's write in brief.
    pub fn run(&self) -> Result<String, String> {
        let unread = |path: &PathBuf, error: std::io::Error| format!("{} does not read: {error}", path.display());
        let (mut scripts, mut catalog) = (Vec::new(), Catalog::default());
        for given in &self.paths {
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
        let graph = Graph::of(scripts);
        let argument = self.argument.as_deref().unwrap_or_default();
        match self.verb {
            Verb::Graph => Ok(graph.printed(self.json)),
            Verb::Outline => {
                let at = graph.writes.iter().position(|write| write.id == argument);
                let at = at.ok_or_else(|| format!("no script has the write {argument}"))?;
                let lineage = Lineage::of(&graph, catalog, self.dialect);
                Ok(self.outline(&graph.writes[at], &lineage.answers[at]))
            }
            Verb::Trace | Verb::Uses => {
                // The argument's last part is its column, the rest its relation.
                let tokens = polyglot_sql::Dialect::get(self.dialect.polyglot()).tokenize(argument).unwrap_or_default();
                let dot = tokens.iter().rposition(|token| token.token_type == TokenType::Dot);
                let dot = dot.ok_or_else(|| format!("{argument:?} is not <relation>.<column>"))?;
                let chars: Vec<char> = argument.chars().collect();
                let (relation, column) = (&chars[..tokens[dot].span.start], &chars[tokens[dot].span.end..]);
                let defaults = self.defaults.read(self.dialect)?;
                let relation = RelationName::read(&relation.iter().collect::<String>(), self.dialect, &defaults)?;
                let from = End { relation, column: Name::read(&column.iter().collect::<String>(), self.dialect)? };
                let lineage = Lineage::of(&graph, catalog, self.dialect);
                let walk = lineage.walk(&from, self.verb == Verb::Trace);
                Ok(walk.ok_or_else(|| format!("no script knows the column {from}"))?.printed(self.json))
            }
        }
    }

    /// `write`, whose answer is `answer`, in brief: as JSON, or one field per line, then its
    /// answer's edges, gaps and refusal, then the words and the place of each of its clauses.
    pub fn outline(&self, write: &Scripted, answer: &Answer) -> String {
        let mode = if write.replaces { "replace" } else { "contribute" };
        if self.json {
            let clauses =
                write.clauses.iter().map(|(words, place)| json!({"keyword": words, "place": place.to_string()}));
            let clauses: Vec<_> = clauses.collect();
            let (id, target, place) = (&write.id, &write.target, write.place.to_string());
            return json!({
                "id": id, "target": target, "mode": mode, "place": place, "clauses": clauses, "answer": answer,
            })
            .to_string();
        }
        let names = |columns: &Vec<Name>| columns.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        let fields = [
            format!("id: {}", write.id),
            format!("target: {}", write.target),
            format!("mode: {mode}"),
            format!("place: {}", write.place),
            format!("columns: {}", answer.columns.as_ref().map_or_else(|| "-".to_string(), names)),
        ];
        let edges = answer.edges.iter().map(|edge| format!("edge: {edge}"));
        let gaps = answer.gaps.iter().map(|gap| format!("gap: {gap}"));
        let refusal = answer.refusal.iter().map(|refusal| format!("refused: {refusal}"));
        let clauses = write.clauses.iter().map(|(words, place)| format!("clause: {words} {place}"));
        fields.into_iter().chain(edges).chain(gaps).chain(refusal).chain(clauses).collect::<Vec<_>>().join("\n")
    }
}
