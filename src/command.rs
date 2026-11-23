// SPDX-License-Identifier: EUPL-1.2
//! The commands, as the command line names them, and what they print.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::dialect::Dialect;
use crate::graph::Graph;
use crate::relation::{Defaults, RelationName};
use crate::script::{Finding, Script};

/// The command `graph` as the command line gives it: its dialect, its defaults, whether it
/// prints JSON, and its paths, each a file, or a directory whose files ending `.sql` it
/// reads.
pub struct Command {
    dialect: Dialect,
    defaults: Defaults,
    json: bool,
    paths: Vec<PathBuf>,
}

impl Command {
    /// The command `graph` that `arguments`, those after its name, give: `--dialect`, a
    /// dialect of the build; `--database` and `--schema`, the defaults, a database without
    /// a schema taking the dialect's usual one; `--json`; and its paths, at least one.
    /// Refused for an option that it does not know, or one without its value.
    pub fn read(arguments: &[OsString]) -> Result<Command, String> {
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
                Some(option) if option.starts_with("--") => return Err(format!("graph has no option {option}")),
                _ => paths.push(PathBuf::from(argument)),
            }
        }
        let name = dialect.ok_or("graph needs --dialect")?;
        let dialect = (Dialect::named(&name))
            .ok_or_else(|| format!("the dialect {name:?} is not one of this build's: {:?}", Dialect::built()))?;
        let defaults = Defaults::given(database, schema, dialect);
        defaults.read(dialect)?;
        if paths.is_empty() {
            return Err("graph needs a path".to_string());
        }
        Ok(Command { dialect, defaults, json, paths })
    }

    /// What the command prints, or why it cannot run: a path that does not read. Each path
    /// is read in the order given, a directory's files sorted by path, into the graph of
    /// their writes: as JSON, or one line per write in migration order,
    /// `<id> <target> <- <relation>, ...`; one per finding, after its kind and a colon; then
    /// the number of statements skipped.
    pub fn run(&self) -> Result<String, String> {
        let unread = |path: &PathBuf, error: std::io::Error| format!("{} does not read: {error}", path.display());
        let mut scripts = Vec::new();
        for given in &self.paths {
            let (mut pending, mut files) = (vec![given.clone()], Vec::new());
            while let Some(path) = pending.pop() {
                if path.is_dir() {
                    let entries = std::fs::read_dir(&path).map_err(|error| unread(&path, error))?;
                    let entries = entries.map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>();
                    pending.extend(entries.map_err(|error| unread(&path, error))?);
                } else if path == *given || path.extension().is_some_and(|extension| extension == "sql") {
                    files.push(path);
                }
            }
            files.sort();
            for file in files {
                let bytes = std::fs::read(&file).map_err(|error| unread(&file, error))?;
                scripts.push(Script::read(&file.display().to_string(), &bytes, self.dialect, self.defaults.clone()));
            }
        }
        let graph = Graph::of(scripts);
        if self.json {
            return Ok(serde_json::to_string(&graph).expect("a graph serialises"));
        }
        let listed = |items: &[String]| items.join(", ");
        let names = |relations: &[RelationName]| listed(&relations.iter().map(ToString::to_string).collect::<Vec<_>>());
        let writes = (graph.writes.iter())
            .map(|write| format!("{} {} <- {}", write.id, write.target, names(&write.reads)).trim_end().to_string());
        let line = |detail: &str| detail.split_whitespace().collect::<Vec<_>>().join(" ");
        let findings = graph.findings.iter().map(|finding| match finding {
            Finding::Refused { place, detail } => format!("refused: {place} {}", line(detail)),
            Finding::NotRead { place, detail } => format!("not_read: {place} {}", line(detail)),
            Finding::Cycle { writes, relations } => format!("cycle: {} ({})", listed(writes), names(relations)),
            Finding::ReplacedTwice { relation, writes } => format!("replaced_twice: {relation} ({})", listed(writes)),
        });
        let skipped = format!("skipped: {}", graph.skipped);
        Ok(writes.chain(findings).chain([skipped]).collect::<Vec<_>>().join("\n"))
    }
}
