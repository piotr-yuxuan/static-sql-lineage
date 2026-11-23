// SPDX-License-Identifier: EUPL-1.2
//! The commands, as the command line names them, and what they print.

use std::ffi::OsString;
use std::path::PathBuf;

use polyglot_sql::TokenType;
use serde_json::{Value, json};

use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::edge::{Edge, End};
use crate::graph::Graph;
use crate::lineage::Lineage;
use crate::relation::{Defaults, Name, RelationName};
use crate::script::{Finding, Script};

/// A command as the command line gives it: its name, its dialect, its defaults, whether it
/// prints JSON, its argument, for all but `graph`, and its paths, each a file, or a directory
/// whose files ending `.sql` it reads.
pub struct Command {
    name: String,
    dialect: Dialect,
    defaults: Defaults,
    json: bool,
    argument: Option<String>,
    paths: Vec<PathBuf>,
}

impl Command {
    /// The names of the commands.
    pub const NAMES: [&str; 3] = ["graph", "trace", "uses"];

    /// The command `name` that `arguments`, those after its name, give: `--dialect`, a
    /// dialect of the build; `--database` and `--schema`, the defaults, a database without
    /// a schema taking the dialect's usual one; `--json`; then, but for `graph`, its
    /// argument; and its paths, at least one. Refused for an option that it does not know,
    /// or one without its value.
    pub fn read(name: &str, arguments: &[OsString]) -> Result<Command, String> {
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
        let dialect = (Dialect::named(&given))
            .ok_or_else(|| format!("the dialect {given:?} is not one of this build's: {:?}", Dialect::built()))?;
        let defaults = Defaults::given(database, schema, dialect);
        defaults.read(dialect)?;
        let graph = name == "graph";
        if paths.len() < 2 - usize::from(graph) {
            return Err(format!("{name} needs {}", if graph { "a path" } else { "its argument and a path" }));
        }
        let argument = (!graph).then(|| paths.remove(0).to_string_lossy().into_owned());
        Ok(Command { name: name.to_string(), dialect, defaults, json, argument, paths })
    }

    /// What the command prints, or why it cannot run: a path that does not read, or an
    /// argument that names no column of the scripts. Each path is read in the order given,
    /// a directory's files ending `.sql`, in any case, sorted by path, into the graph of their
    /// writes and the catalog of their declarations. As JSON, or as text: for `graph`, one
    /// line per write in migration order, `<id> <target> <- <relation>, ...`, one per finding,
    /// after its kind and a colon, then the number of statements skipped; for `trace` and
    /// `uses`, one line per edge of the walk from the argument's column, upstream or
    /// downstream, `<relation>.<column> <kind> <id>, ...` of the end that it reaches, indented
    /// by its depth, then the gaps, the refusals and the cycles that it meets.
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
        let listed = |items: &[String]| items.join(", ");
        // A value's text, as its JSON spells it, `-` for none.
        let named = |value: Value| value.as_str().unwrap_or("-").to_string();
        let argument = self.argument.as_deref().unwrap_or_default();
        let (json, lines): (String, Vec<String>) = match self.name.as_str() {
            "graph" => {
                let names =
                    |relations: &[RelationName]| listed(&relations.iter().map(ToString::to_string).collect::<Vec<_>>());
                let writes = (graph.writes.iter()).map(|write| {
                    format!("{} {} <- {}", write.id, write.target, names(&write.reads)).trim_end().to_string()
                });
                let line = |detail: &str| detail.split_whitespace().collect::<Vec<_>>().join(" ");
                let findings = graph.findings.iter().map(|finding| match finding {
                    Finding::Refused { place, detail } => format!("refused: {place} {}", line(detail)),
                    Finding::NotRead { place, detail } => format!("not_read: {place} {}", line(detail)),
                    Finding::DeclaredTwice { place, relation } => format!("declared_twice: {place} {relation}"),
                    Finding::Cycle { writes, relations } => format!("cycle: {} ({})", listed(writes), names(relations)),
                    Finding::ReplacedTwice { relation, writes } => {
                        format!("replaced_twice: {relation} ({})", listed(writes))
                    }
                });
                let skipped = format!("skipped: {}", graph.skipped);
                (
                    serde_json::to_string(&graph).expect("a graph serialises"),
                    writes.chain(findings).chain([skipped]).collect(),
                )
            }
            _ => {
                // The argument's last part is its column, the rest its relation.
                let tokens = polyglot_sql::Dialect::get(self.dialect.polyglot()).tokenize(argument).unwrap_or_default();
                let dot = tokens.iter().rposition(|token| token.token_type == TokenType::Dot);
                let dot = dot.ok_or_else(|| format!("{argument:?} is not <relation>.<column>"))?;
                let chars: Vec<char> = argument.chars().collect();
                let (relation, column) = (&chars[..tokens[dot].span.start], &chars[tokens[dot].span.end..]);
                let defaults = self.defaults.read(self.dialect)?;
                let relation = RelationName::read(&relation.iter().collect::<String>(), self.dialect, &defaults)?;
                let from = End { relation, column: Name::read(&column.iter().collect::<String>(), self.dialect)? };
                let lineage = Lineage::of(&graph.writes, catalog, self.dialect);
                let upstream = self.name == "trace";
                let walk = lineage.walk(&from, upstream).ok_or_else(|| format!("no script knows the column {from}"))?;
                let reached = |edge: &Edge| if upstream { edge.parent.to_string() } else { edge.child.to_string() };
                let edges = walk.edges.iter().map(|(edge, writes, depth)| {
                    let kind = named(json!(edge.kind));
                    format!("{}{} {kind} {}", "  ".repeat(depth - 1), reached(edge), listed(writes))
                });
                let gaps = walk.gaps.iter().map(|(write, gap)| {
                    let (relation, column) = (named(json!(gap.relation)), named(json!(gap.column)));
                    format!("gap: {write} {} {relation} {column}", named(json!(gap.reason)))
                });
                let refusals = walk
                    .refusals
                    .iter()
                    .map(|(write, refusal)| format!("refused: {write} {}", named(json!(refusal.reason))));
                let cycles = (walk.cycles.iter()).map(|cycle| {
                    format!("cycle: {}", listed(&cycle.iter().map(ToString::to_string).collect::<Vec<_>>()))
                });
                let json = json!({
                    "column": from,
                    "edges": walk.edges.iter().map(|(edge, writes, depth)| {
                        let (parent, child, kind) = (&edge.parent, &edge.child, edge.kind);
                        json!({"parent": parent, "child": child, "kind": kind, "writes": writes, "depth": depth})
                    }).collect::<Vec<_>>(),
                    "gaps": walk.gaps.iter().map(|(write, gap)| {
                        json!({"write": write, "reason": gap.reason, "relation": gap.relation, "column": gap.column})
                    }).collect::<Vec<_>>(),
                    "refusals": walk.refusals.iter().map(|(write, refusal)| {
                        json!({"write": write, "reason": refusal.reason})
                    }).collect::<Vec<_>>(),
                    "cycles": walk.cycles,
                });
                (json.to_string(), edges.chain(gaps).chain(refusals).chain(cycles).collect())
            }
        };
        Ok(if self.json { json } else { lines.join("\n") })
    }
}
