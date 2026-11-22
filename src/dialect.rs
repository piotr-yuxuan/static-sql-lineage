//! The dialects of this build.

use serde::Serialize;

/// A dialect of this build: the grammar, and the rules for quoting and case, under
/// which a request is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Snowflake,
    Tsql,
    Duckdb,
}

impl Dialect {
    /// The dialects whose Cargo feature is on, in a fixed order.  polyglot-sql parses
    /// every dialect's name whatever its features, and silently treats one left out as
    /// generic, so this alone says which dialects are built.
    pub fn built() -> Vec<Dialect> {
        [
            (cfg!(feature = "snowflake"), Dialect::Snowflake),
            (cfg!(feature = "tsql"), Dialect::Tsql),
            (cfg!(feature = "duckdb"), Dialect::Duckdb),
        ]
        .into_iter()
        .filter_map(|(built, dialect)| built.then_some(dialect))
        .collect()
    }
}
