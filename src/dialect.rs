// SPDX-License-Identifier: EUPL-1.2
//! The dialects of this build.

use polyglot_sql::DialectType;
use polyglot_sql::optimizer::normalize_identifiers::{NormalizationStrategy, get_normalization_strategy};
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

    /// The dialect of this build that `--describe` spells `name`, if any.
    pub fn named(name: &str) -> Option<Dialect> {
        Dialect::built().into_iter().find(|dialect| serde_json::json!(dialect) == name)
    }

    /// The dialect as polyglot-sql names it.
    pub fn polyglot(self) -> DialectType {
        match self {
            Dialect::Snowflake => DialectType::Snowflake,
            Dialect::Tsql => DialectType::TSQL,
            Dialect::Duckdb => DialectType::DuckDB,
        }
    }

    /// The schema of a database given without one: SQL Server's `dbo`, Snowflake's
    /// `PUBLIC`, DuckDB's `main`.
    pub fn default_schema(self) -> &'static str {
        match self {
            Dialect::Tsql => "dbo",
            Dialect::Snowflake => "PUBLIC",
            Dialect::Duckdb => "main",
        }
    }

    /// The rule by which a name is normalised: polyglot-sql's for the dialect, but for
    /// T-SQL, whose names ignore case whether delimited or not, as SQL Server's default
    /// collation compares them.
    pub fn strategy(self) -> NormalizationStrategy {
        match self {
            Dialect::Tsql => NormalizationStrategy::CaseInsensitive,
            Dialect::Snowflake | Dialect::Duckdb => get_normalization_strategy(Some(self.polyglot())),
        }
    }

    /// The text that polyglot-sql reads for `sql`: under T-SQL, `sql` lowered whole,
    /// since polyglot-sql keeps a delimited name's case where the dialect ignores it, as
    /// it ignores the case of keywords, and no edge depends on a literal's; under the
    /// others, `sql` itself.
    pub fn polyglot_text(self, sql: &str) -> String {
        match self {
            Dialect::Tsql => sql.to_lowercase(),
            Dialect::Snowflake | Dialect::Duckdb => sql.to_string(),
        }
    }
}
