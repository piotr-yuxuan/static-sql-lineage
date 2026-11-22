//! What this build is, as `--describe` says it.

use serde::Serialize;

use crate::dialect::Dialect;

/// The engine, by its name in `Cargo.lock`.
const ENGINE: &str = "polyglot-sql";

/// What this build is: its version, the protocols it serves, its engine and its
/// dialects.
#[derive(Serialize)]
pub struct Description {
    tool: &'static str,
    version: &'static str,
    /// None until a request is read.
    protocols: Vec<u32>,
    engine: Engine,
    dialects: Vec<Dialect>,
}

/// The SQL engine of this build, as `Cargo.lock` pins it.
#[derive(Serialize)]
pub struct Engine {
    name: &'static str,
    version: String,
    /// Verbatim, so that a fork would show its commit.
    source: String,
}

impl Description {
    /// This build's description, its engine read from `lock`, its `Cargo.lock`.
    pub fn of_build(lock: &str) -> Description {
        let entry = format!("name = \"{ENGINE}\"");
        let package = lock.split("[[package]]").find(|package| package.lines().any(|line| line == entry));
        let field = |key: &str| {
            let value =
                |line: &str| Some(line.strip_prefix(key)?.strip_prefix(" = \"")?.strip_suffix('"')?.to_string());
            package.and_then(|package| package.lines().find_map(value)).unwrap_or_default()
        };
        Description {
            tool: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            protocols: Vec::new(),
            engine: Engine { name: ENGINE, version: field("version"), source: field("source") },
            dialects: Dialect::built(),
        }
    }
}
