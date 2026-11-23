// SPDX-License-Identifier: EUPL-1.2
//! A write as the request gives it: its query, the relations it reads, and the names of
//! its output columns.

use polyglot_sql::expressions::Expression;
use polyglot_sql::parse_one;
use polyglot_sql::scope::{SourceKind, build_scope};
use serde::Deserialize;

use crate::answer::{Refusal, RefusalReason};
use crate::dialect::Dialect;
use crate::relation::{Name, RelationName, repeated};

/// The request's form of a write: its id, its target and the SQL of its query.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub id: String,
    pub target: String,
    pub sql: String,
}

/// The names of a write's output columns, or, when a `*` leaves them unknown, the
/// relation that it reads, if any.
pub enum Outputs {
    Named(Vec<Name>),
    Unknown(Option<String>),
}

impl Write {
    /// The write's query, the outermost one when it is parenthesised, as the engine
    /// reads it; refused when it is not one statement, or not a query.
    pub fn query(&self, dialect: Dialect) -> Result<Expression, Refusal> {
        let parsed = parse_one(&self.sql, dialect.polyglot())
            .map_err(|error| Refusal { reason: RefusalReason::Unparsed, detail: error.to_string() })?;
        match parsed {
            Expression::Subquery(subquery) if subquery.alias.is_none() => Ok(subquery.this),
            query @ (Expression::Select(_)
            | Expression::Union(_)
            | Expression::Intersect(_)
            | Expression::Except(_)) => Ok(query),
            _ => Err(Refusal { reason: RefusalReason::NotAQuery, detail: "a statement, not a query".to_string() }),
        }
    }

    /// The relations that `query` reads: its table references that name no CTE in scope,
    /// as the engine scopes them, ordered by their spelling.
    pub fn relations(&self, query: &Expression, dialect: Dialect, defaults: &[Name]) -> Vec<RelationName> {
        let sql: Vec<char> = self.sql.chars().collect();
        let root = build_scope(query);
        let (mut scopes, mut relations) = (vec![&root], Vec::new());
        while let Some(scope) = scopes.pop() {
            relations.extend(scope.sources.values().filter_map(|source| match (&source.kind, &source.expression) {
                (SourceKind::Table, Expression::Table(table)) => RelationName::of(table, &sql, dialect, defaults),
                _ => None,
            }));
            let children =
                [&scope.subquery_scopes, &scope.derived_table_scopes, &scope.cte_scopes, &scope.union_scopes];
            scopes.extend(children.into_iter().chain([&scope.udtf_scopes]).flatten());
        }
        relations.sort_by_key(ToString::to_string);
        relations
    }

    /// The names of `query`'s output columns, from its outermost select list, or the
    /// left branch's for a set operation: an alias names its column, so does a column;
    /// refused when one is unnamed, or two are named alike.
    pub fn outputs(&self, query: &Expression, dialect: Dialect) -> Result<Outputs, Refusal> {
        let (sql, mut query): (Vec<char>, _) = (self.sql.chars().collect(), query);
        let select = loop {
            query = match query {
                Expression::Union(set) => &set.left,
                Expression::Intersect(set) => &set.left,
                Expression::Except(set) => &set.left,
                Expression::Subquery(subquery) => &subquery.this,
                Expression::Select(select) => break select,
                _ => return Ok(Outputs::Unknown(None)),
            }
        };
        if select.expressions.iter().any(|expression| matches!(expression, Expression::Star(_))) {
            return Ok(Outputs::Unknown(None));
        }
        let names: Vec<Option<Name>> = (select.expressions.iter())
            .map(|expression| match expression {
                Expression::Alias(alias) => Name::of(&alias.alias, &sql, dialect),
                Expression::Column(column) => Name::of(&column.name, &sql, dialect),
                _ => None,
            })
            .collect();
        if let Some(place) = names.iter().position(Option::is_none) {
            let detail = format!("output column {} has no name", place + 1);
            return Err(Refusal { reason: RefusalReason::UnnamedColumn, detail });
        }
        let names: Vec<Name> = names.into_iter().flatten().collect();
        if let Some(name) = repeated(&names) {
            let detail = format!("two output columns are named {name}");
            return Err(Refusal { reason: RefusalReason::DuplicateColumn, detail });
        }
        Ok(Outputs::Named(names))
    }
}
