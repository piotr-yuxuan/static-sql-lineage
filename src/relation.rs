// SPDX-License-Identifier: EUPL-1.2
//! The names of relations and of their columns: SQL text under the dialect.

use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};

use polyglot_sql::expressions::{Expression, Identifier, TableRef};
use polyglot_sql::optimizer::normalize_identifiers::{get_normalization_strategy, normalize_identifier};
use polyglot_sql::parse_one;
use serde::{Deserialize, Serialize, Serializer};

use crate::dialect::Dialect;

/// One identifier as SQL text under the dialect: its spelling, its text unquoted, and
/// its form normalised by the dialect's rules, on which equality is.
#[derive(Clone, Debug)]
pub struct Name {
    spelling: String,
    text: String,
    normalised: String,
}

impl Name {
    /// The name that `text` spells, read alone as one identifier.
    pub fn read(text: &str, dialect: Dialect) -> Result<Name, String> {
        let parts = RelationName::read(text, dialect, &[]).map(|relation| relation.parts);
        match parts.as_deref() {
            Ok([name]) => Ok(name.clone()),
            _ => Err(format!("{text:?} is not one identifier")),
        }
    }

    /// The name of `identifier`, parsed from `sql`, spelled as `sql` spells it; none for
    /// an identifier that the parser made rather than read.
    pub fn of(identifier: &Identifier, sql: &[char], dialect: Dialect) -> Option<Name> {
        let span = identifier.span?;
        let strategy = get_normalization_strategy(Some(dialect.polyglot()));
        Some(Name {
            spelling: sql.get(span.start..span.end)?.iter().collect(),
            text: identifier.name.clone(),
            normalised: normalize_identifier(identifier.clone(), strategy).name,
        })
    }

    /// Whether the engine's text for a name, which drops quotes, is this name's: the one
    /// rule that ignores case, for the engine's reports alone.
    pub fn reported_as(&self, text: &str) -> bool {
        self.text.to_lowercase() == text.to_lowercase()
    }

    /// The normalised form, quoted, so that the engine keeps it as it is.
    pub fn quoted(&self) -> String {
        format!("\"{}\"", self.normalised.replace('"', "\"\""))
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Name) -> bool {
        self.normalised == other.normalised
    }
}

impl Eq for Name {}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.normalised.hash(state);
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.spelling)
    }
}

impl Serialize for Name {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// The name of a relation, of one to three parts: its spelling, its parts as written,
/// and its parts completed by the request's defaults, on which equality is.
#[derive(Clone, Debug)]
pub struct RelationName {
    spelling: String,
    text: String,
    parts: Vec<Name>,
}

impl RelationName {
    /// The relation that `text` names, a bare name read as the table of
    /// `SELECT * FROM <text>`, with no text outside its parts.
    pub fn read(text: &str, dialect: Dialect, defaults: &[Name]) -> Result<RelationName, String> {
        let sql = format!("SELECT * FROM {text}");
        let refused = || format!("{text:?} is not a bare name");
        let Ok(Expression::Select(select)) = parse_one(&sql, dialect.polyglot()) else { return Err(refused()) };
        let Some(Expression::Table(table)) = select.from.as_ref().and_then(|from| from.expressions.first()) else {
            return Err(refused());
        };
        let chars: Vec<char> = sql.chars().collect();
        RelationName::of(table, &chars, dialect, defaults)
            .filter(|relation| relation.spelling == text)
            .ok_or_else(refused)
    }

    /// The relation that `table`, parsed from `sql`, names, its missing schema then its
    /// missing database taken from `defaults`, the names that complete a name of one
    /// part, outermost first.
    pub fn of(table: &TableRef, sql: &[char], dialect: Dialect, defaults: &[Name]) -> Option<RelationName> {
        let written: Vec<Name> = [table.catalog.as_ref(), table.schema.as_ref(), Some(&table.name)]
            .into_iter()
            .flatten()
            .map(|part| Name::of(part, sql, dialect))
            .collect::<Option<_>>()?;
        let joined = |part: fn(&Name) -> &str| written.iter().map(part).collect::<Vec<_>>().join(".");
        let (spelling, text) = (joined(|name| name.spelling.as_str()), joined(|name| name.text.as_str()));
        let missing = defaults.len().saturating_sub(written.len() - 1);
        Some(RelationName { spelling, text, parts: defaults[..missing].iter().cloned().chain(written).collect() })
    }

    /// Whether the engine's text for a relation is this name's parts as written, by the
    /// rule of [`Name::reported_as`].
    pub fn reported_as(&self, text: &str) -> bool {
        self.text.to_lowercase() == text.to_lowercase()
    }

    /// Whether two relations differ although the engine's text for one is the other's.
    pub fn alike(&self, other: &RelationName) -> bool {
        self != other && self.reported_as(&other.text)
    }

    /// The parts, completed, each as [`Name::quoted`] gives it.
    pub fn quoted(&self) -> Vec<String> {
        self.parts.iter().map(Name::quoted).collect()
    }
}

impl PartialEq for RelationName {
    fn eq(&self, other: &RelationName) -> bool {
        self.parts == other.parts
    }
}

impl Eq for RelationName {}

impl Hash for RelationName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.parts.hash(state);
    }
}

impl fmt::Display for RelationName {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.spelling)
    }
}

/// The request's default database and schema, as it gives them.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    database: Option<String>,
    schema: Option<String>,
}

impl Defaults {
    /// The names that complete a name of one part, outermost first.
    pub fn read(&self, dialect: Dialect) -> Result<Vec<Name>, String> {
        if self.database.is_some() && self.schema.is_none() {
            return Err("the default database has no default schema".to_string());
        }
        [&self.database, &self.schema].into_iter().flatten().map(|text| Name::read(text, dialect)).collect()
    }
}

/// The first of `items` that an earlier one equals.
pub fn repeated<T: Eq + Hash + Copy>(items: impl IntoIterator<Item = T>) -> Option<T> {
    let mut seen = HashSet::new();
    items.into_iter().find(|item| !seen.insert(*item))
}
