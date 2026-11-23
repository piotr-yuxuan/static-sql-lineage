// SPDX-License-Identifier: EUPL-1.2
//! A write as the request gives it: its query, the relations it reads, and the names of
//! its output columns.

use polyglot_sql::expressions::{Cte, Expression, Select};
use polyglot_sql::parse_one;
use polyglot_sql::scope::{Scope, SourceKind, build_scope};
use serde::Deserialize;

use crate::answer::{Refusal, RefusalReason};
use crate::catalog::Catalog;
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
        let sources = scopes(&root).into_iter().flat_map(|(_, scope)| scope.sources.values());
        let mut relations: Vec<RelationName> =
            (sources.filter_map(|source| match (&source.kind, &source.expression) {
                (SourceKind::Table, Expression::Table(table)) => RelationName::of(table, &sql, dialect, defaults),
                _ => None,
            }))
            .collect();
        relations.sort_by_key(ToString::to_string);
        relations
    }

    /// The names of `query`'s output columns, as [`named`] gives them; refused when one
    /// is unnamed, or two are named alike.
    pub fn outputs(
        &self,
        query: &Expression,
        catalog: &Catalog,
        dialect: Dialect,
        defaults: &[Name],
    ) -> Result<Outputs, Refusal> {
        let sql: Vec<char> = self.sql.chars().collect();
        let names = match named(query, &[], &sql, catalog, dialect, defaults) {
            Ok(names) => names,
            Err(relation) => return Ok(Outputs::Unknown(relation)),
        };
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

/// Every scope of `root`, the engine's scopes of a query, with its path as the engine
/// writes it, such as `root.ctes[0].subqueries[1]`.
fn scopes(root: &Scope) -> Vec<(String, &Scope)> {
    let (mut pending, mut found) = (vec![("root".to_string(), root)], Vec::new());
    while let Some((path, scope)) = pending.pop() {
        let kinds = [
            ("subqueries", &scope.subquery_scopes),
            ("derived", &scope.derived_table_scopes),
            ("ctes", &scope.cte_scopes),
            ("branches", &scope.union_scopes),
            ("udtfs", &scope.udtf_scopes),
        ];
        for (kind, children) in kinds {
            pending.extend(children.iter().enumerate().map(|(at, child)| (format!("{path}.{kind}[{at}]"), child)));
        }
        found.push((path, scope));
    }
    found
}

/// The names of the columns that `query` gives, `None` for one that it leaves unnamed;
/// or, when a `*` leaves them unknown, the relation that it reads, if any. A query gives
/// those of its outermost select list, or its left branch's, as [`listed`] names them; a
/// CTE among `ctes`, those in scope, gives its query's, and a relation of the catalog its
/// columns, as the catalog spells them. A CTE with a list of columns, or a source with
/// column aliases, leaves them unknown.
fn named<'a>(
    query: &'a Expression,
    ctes: &[&'a Cte],
    sql: &[char],
    catalog: &Catalog,
    dialect: Dialect,
    defaults: &[Name],
) -> Result<Vec<Option<Name>>, Option<String>> {
    let (mut query, mut ctes) = (query, ctes.to_vec());
    loop {
        let (with, inner) = match query {
            Expression::Select(select) => {
                ctes.extend(select.with.iter().flat_map(|with| &with.ctes));
                return listed(select, &ctes, sql, catalog, dialect, defaults);
            }
            Expression::Union(set) => (&set.with, &set.left),
            Expression::Intersect(set) => (&set.with, &set.left),
            Expression::Except(set) => (&set.with, &set.left),
            Expression::Subquery(subquery) if subquery.column_aliases.is_empty() => (&None, &subquery.this),
            Expression::Table(table) if table.column_aliases.is_empty() => {
                let cte = ctes
                    .iter()
                    .rposition(|cte| table.schema.is_none() && cte.alias.name.eq_ignore_ascii_case(&table.name.name));
                let Some(at) = cte else {
                    let relation = RelationName::of(table, sql, dialect, defaults).ok_or(None)?;
                    let (_, columns) = catalog.get(&relation).ok_or_else(|| Some(relation.to_string()))?;
                    return Ok(columns.iter().cloned().map(Some).collect());
                };
                let cte: &Cte = Some(ctes[at]).filter(|cte| cte.columns.is_empty()).ok_or(None)?;
                ctes.truncate(at);
                (&None, &cte.this)
            }
            _ => return Err(None),
        };
        ctes.extend(with.iter().flat_map(|with| &with.ctes));
        query = inner;
    }
}

/// The names of the columns of `select`'s list: an alias names its column, so does a
/// column, and a `*` gives those of the one source that its qualifier names, or of the
/// only source of `select`, as [`named`] gives them, less those that it excludes, as it
/// renames them. Unknown when a `*` reads several sources, or reaches an unnamed column.
fn listed<'a>(
    select: &'a Select,
    ctes: &[&'a Cte],
    sql: &[char],
    catalog: &Catalog,
    dialect: Dialect,
    defaults: &[Name],
) -> Result<Vec<Option<Name>>, Option<String>> {
    let sources: Vec<&Expression> = (select.from.iter().flat_map(|from| &from.expressions))
        .chain(select.joins.iter().map(|join| &join.this))
        .collect();
    let mut names = Vec::new();
    for expression in &select.expressions {
        let Expression::Star(star) = expression else {
            names.push(match expression {
                Expression::Alias(alias) => Name::of(&alias.alias, sql, dialect),
                Expression::Column(column) => Name::of(&column.name, sql, dialect),
                _ => None,
            });
            continue;
        };
        let called = |source: &&Expression| match (&star.table, source) {
            (None, _) => sources.len() == 1,
            (Some(qualifier), Expression::Table(table)) => {
                table.alias.as_ref().unwrap_or(&table.name).name.eq_ignore_ascii_case(&qualifier.name)
            }
            (Some(qualifier), Expression::Subquery(subquery)) => {
                subquery.alias.as_ref().is_some_and(|alias| alias.name.eq_ignore_ascii_case(&qualifier.name))
            }
            _ => false,
        };
        let source = sources.iter().copied().find(called).ok_or(None)?;
        let mut columns = named(source, ctes, sql, catalog, dialect, defaults)?;
        if columns.iter().any(Option::is_none) {
            return Err(None);
        }
        let excluded: Vec<Option<Name>> =
            star.except.iter().flatten().map(|identifier| Name::of(identifier, sql, dialect)).collect();
        columns.retain(|column| !excluded.contains(column));
        for (old, new) in star.rename.iter().flatten() {
            let old = Name::of(old, sql, dialect);
            columns
                .iter_mut()
                .filter(|column| **column == old)
                .for_each(|column| *column = Name::of(new, sql, dialect));
        }
        names.extend(columns);
    }
    Ok(names)
}
