// SPDX-License-Identifier: EUPL-1.2
//! The dialects of this build.

use polyglot_sql::optimizer::normalize_identifiers::{NormalizationStrategy, get_normalization_strategy};
use polyglot_sql::{DialectType, Token};
use serde::Serialize;

/// A dialect of this build: the grammar, and the rules for quoting and case, under
/// which a request is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Snowflake,
    Tsql,
    Duckdb,
    Postgresql,
    Redshift,
    Databricks,
}

/// What sets a dialect apart: its name, as `--describe` spells it; polyglot-sql's name for
/// it; the schema of a database given without one; whether its text is lowered, its names
/// ignoring case, delimited or not; whether a `*` over `USING` or `NATURAL` lists the keys
/// that the join merges first.
struct Rules {
    name: &'static str,
    polyglot: DialectType,
    schema: &'static str,
    lowered: bool,
    keys_first: bool,
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
            (cfg!(feature = "postgresql"), Dialect::Postgresql),
            (cfg!(feature = "redshift"), Dialect::Redshift),
            (cfg!(feature = "databricks"), Dialect::Databricks),
        ]
        .into_iter()
        .filter_map(|(built, dialect)| built.then_some(dialect))
        .collect()
    }

    /// The row of this dialect's rules.
    fn rules(self) -> Rules {
        // Name, polyglot-sql's name, default schema, lowered, keys first.
        let rules = |name, polyglot, schema, lowered, keys_first| Rules { name, polyglot, schema, lowered, keys_first };
        match self {
            Dialect::Snowflake => rules("snowflake", DialectType::Snowflake, "PUBLIC", false, true),
            // SQL Server's default collation compares names ignoring case, delimited or not.
            Dialect::Tsql => rules("tsql", DialectType::TSQL, "dbo", true, false),
            Dialect::Duckdb => rules("duckdb", DialectType::DuckDB, "main", false, false),
            Dialect::Postgresql => rules("postgresql", DialectType::PostgreSQL, "public", false, true),
            // Names ignore case, with `enable_case_sensitive_identifier` off, as by default.
            // Redshift's documentation says nothing of the order of a `*` over `USING`: the
            // SQL standard's, for a joined table, puts the merged keys first, as PostgreSQL does.
            Dialect::Redshift => rules("redshift", DialectType::Redshift, "public", true, true),
            Dialect::Databricks => rules("databricks", DialectType::Databricks, "default", true, true),
        }
    }

    /// The dialect's name, as `--describe` spells it.
    pub fn name(self) -> &'static str {
        self.rules().name
    }

    /// The dialect of this build that `--describe` spells `name`, or why there is none.
    pub fn named(name: &str) -> Result<Dialect, String> {
        let named = Dialect::built().into_iter().find(|dialect| dialect.name() == name);
        named.ok_or_else(|| format!("the dialect {name:?} is not one of this build's: {:?}", Dialect::built()))
    }

    /// The dialect as polyglot-sql names it.
    pub fn polyglot(self) -> DialectType {
        self.rules().polyglot
    }

    /// polyglot-sql's tokens of `text` under the dialect.
    pub fn tokens(self, text: &str) -> Result<Vec<Token>, polyglot_sql::Error> {
        polyglot_sql::Dialect::get(self.polyglot()).tokenize(text)
    }

    /// The schema of a database given without one.
    pub fn default_schema(self) -> &'static str {
        self.rules().schema
    }

    /// The rule by which a name is normalised: case ignored where the dialect's text is
    /// lowered, polyglot-sql's rule for the dialect elsewhere.
    pub fn strategy(self) -> NormalizationStrategy {
        if self.rules().lowered {
            NormalizationStrategy::CaseInsensitive
        } else {
            get_normalization_strategy(Some(self.polyglot()))
        }
    }

    /// The text that polyglot-sql reads for `sql`: `sql` lowered whole where the dialect's
    /// names ignore case, since polyglot-sql keeps a delimited name's case there, as it
    /// ignores the case of keywords, and no edge depends on a literal's; elsewhere, `sql`
    /// itself.
    pub fn polyglot_text(self, sql: &str) -> String {
        if self.rules().lowered { sql.to_lowercase() } else { sql.to_string() }
    }

    /// Whether a `*` over `USING` or `NATURAL` lists the keys that the join merges first.
    pub fn keys_first(self) -> bool {
        self.rules().keys_first
    }
}
