// SPDX-License-Identifier: EUPL-1.2
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
    /// This build's description, its engine read from `lock`, its `Cargo.lock`.  The
    /// lock is this build's own, so an engine missing from it is a defect of the build.
    pub fn of_build(lock: &str) -> Description {
        let entry = format!("name = \"{ENGINE}\"");
        let package = (lock.split("[[package]]").find(|package| package.lines().any(|line| line == entry)))
            .expect("Cargo.lock has an entry for the engine");
        let field = |key: &str| {
            let value =
                |line: &str| Some(line.strip_prefix(key)?.strip_prefix(" = \"")?.strip_suffix('"')?.to_string());
            package.lines().find_map(value)
        };
        Description {
            tool: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            protocols: Vec::new(),
            engine: Engine {
                name: ENGINE,
                version: field("version").expect("the engine's entry in Cargo.lock has a version"),
                // A path dependency has no source.
                source: field("source").unwrap_or_default(),
            },
            dialects: Dialect::built(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_source_is_verbatim() {
        let lock = "[[package]]\nname = \"polyglot-sql\"\nversion = \"0.14.4\"\n\
                    source = \"git+https://github.com/example/polyglot?rev=abc123#abc123\"\n";
        let engine = Description::of_build(lock).engine;
        assert_eq!(engine.version, "0.14.4");
        assert_eq!(engine.source, "git+https://github.com/example/polyglot?rev=abc123#abc123");
    }

    #[test]
    fn a_path_dependency_has_no_source() {
        let lock = "[[package]]\nname = \"polyglot-sql\"\nversion = \"0.14.3\"\n";
        assert_eq!(Description::of_build(lock).engine.source, "");
    }
}
