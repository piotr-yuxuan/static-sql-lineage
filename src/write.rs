// SPDX-License-Identifier: EUPL-1.2
//! A write as the request gives it: its query, the relations it reads, and the names of
//! its output columns.

use std::collections::HashSet;

use polyglot_sql::expressions::{Cte, Expression, Select};
use polyglot_sql::scope::{Scope, SourceKind, build_scope, walk_in_scope};
use polyglot_sql::validation::{ValidationSchema, mapping_schema_from_validation_schema_with_dialect};
use polyglot_sql::{ColumnUseContext, LineageNode, lineage_at_with_schema, parse_one};
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

/// Where a subquery sits in the `SELECT` that holds it: in a clause, whose uses have the
/// context it gives, or the same clause through a tuple's `IN`, which reads more columns
/// of the subquery than the engine reports, its first; in the select list of the
/// outermost query or of one of its branches, at an output column's place; or in that of
/// a CTE or a derived table, whose name it gives with the column's.
#[derive(Clone)]
pub enum Holder {
    Clause(ColumnUseContext),
    Tuple(ColumnUseContext),
    Output(usize),
    Column(Name, Name),
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
        let sources = scopes(&root, &sql, dialect).into_iter().flat_map(|(_, scope, _)| scope.sources.values());
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

    /// Each subquery of `query` that no subquery holds, by the scope path that the engine
    /// gives it, with its holder; none for one that the engine numbers otherwise, or that
    /// follows a `*` in its select list, where its place would count the `*` as one. The
    /// engine numbers the subqueries of a `SELECT` by its `collect_subqueries`: the fields
    /// that hold them, in the order of its struct, each walked by `walk_in_scope`, finding a
    /// `Subquery` with no alias, the query of `EXISTS` and of `IN`, and that of `ANY` and
    /// `ALL` twice, as a quantifier's and as a `Subquery`.
    pub fn holders(&self, query: &Expression, dialect: Dialect) -> Vec<(String, Option<Holder>)> {
        let sql: Vec<char> = self.sql.chars().collect();
        let root = build_scope(query);
        let mut found = Vec::new();
        for (path, scope, owner) in scopes(&root, &sql, dialect) {
            let query = match &scope.expression {
                Expression::Cte(cte) => &cte.this,
                query => query,
            };
            let Expression::Select(select) = query else { continue };
            if path.contains(".subqueries[") || path.contains(".udtfs[") {
                continue;
            }
            // In the select list, the output column at its place or, in a CTE or a
            // derived table, the column that the list names there.
            let star = select.expressions.iter().position(|expression| matches!(expression, Expression::Star(_)));
            let listed = select.expressions.iter().enumerate().map(|(at, expression)| {
                let holder = owner.as_ref().map_or(Some(Holder::Output(at)), |owner| {
                    Some(Holder::Column(owner.clone(), name(expression, &sql, dialect)?))
                });
                (holder.filter(|_| star.is_none_or(|star| at < star)), expression)
            });
            let clause = |context| Some(Holder::Clause(context));
            let ons = select.joins.iter().flat_map(|join| join.on.iter().chain(&join.match_condition));
            let groups = select.group_by.iter().flat_map(|group| &group.expressions);
            let orders = select.order_by.iter().flat_map(|order| order.expressions.iter().map(|key| &key.this));
            let clauses = (listed.chain(ons.map(|on| (clause(ColumnUseContext::Join), on))))
                .chain(select.where_clause.iter().map(|filter| (clause(ColumnUseContext::Filter), &filter.this)))
                .chain(groups.map(|key| (clause(ColumnUseContext::Group), key)))
                .chain(select.having.iter().map(|having| (clause(ColumnUseContext::Having), &having.this)))
                .chain(select.qualify.iter().map(|qualify| (clause(ColumnUseContext::Qualify), &qualify.this)))
                .chain(orders.map(|key| (clause(ColumnUseContext::Order), key)));
            let held: Vec<(Option<Holder>, &Expression)> = clauses
                .flat_map(|(holder, clause)| {
                    let mut seen = HashSet::new();
                    let held = walk_in_scope(clause, false).filter_map(move |node| {
                        let (query, tuple) = match node {
                            Expression::Subquery(subquery) if subquery.alias.is_none() => (&subquery.this, false),
                            Expression::Exists(exists) => (&exists.this, false),
                            Expression::In(in_) => (in_.query.as_ref()?, matches!(in_.this, Expression::Tuple(_))),
                            Expression::Any(quantified) | Expression::All(quantified) => (&quantified.subquery, false),
                            _ => return None,
                        };
                        // A tuple's `IN` reads more columns of the subquery than the engine reports.
                        match &holder {
                            Some(Holder::Clause(context)) if tuple => Some((Some(Holder::Tuple(*context)), query)),
                            holder => Some((holder.clone(), query)),
                        }
                    });
                    held.filter(move |(_, query)| seen.insert(*query as *const Expression))
                })
                .collect();
            found.extend(scope.subquery_scopes.iter().enumerate().map(|(at, subquery)| {
                let holder = held.get(at).filter(|(_, query)| **query == subquery.expression);
                (format!("{path}.subqueries[{at}]"), holder.and_then(|(holder, _)| holder.clone()))
            }));
        }
        found
    }

    /// The engine's lineage of each of the `outputs` output columns of `query`, against
    /// `schema`, none for one that it does not give; for a write whose `holders` hold a
    /// subquery in a CTE's or a derived table's select list, and no other.
    pub fn steps(
        query: &Expression,
        holders: &[(String, Option<Holder>)],
        schema: &ValidationSchema,
        dialect: Dialect,
        outputs: usize,
    ) -> Vec<Option<LineageNode>> {
        if !holders.iter().any(|(_, holder)| matches!(holder, Some(Holder::Column(..)))) {
            return Vec::new();
        }
        let schema = mapping_schema_from_validation_schema_with_dialect(schema, dialect.polyglot());
        let lineage = |place| lineage_at_with_schema(place, query, Some(&schema), Some(dialect.polyglot()), false);
        (0..outputs).map(|place| lineage(place).ok()).collect()
    }
}

/// Every scope of `root`, the engine's scopes of a query, with its path as the engine
/// writes it, such as `root.ctes[0].subqueries[1]`, and the name of the CTE or derived
/// table that it is, or is a branch of.
fn scopes<'a>(root: &'a Scope, sql: &[char], dialect: Dialect) -> Vec<(String, &'a Scope, Option<Name>)> {
    let (mut pending, mut found) = (vec![("root".to_string(), root, None)], Vec::new());
    while let Some((path, scope, owner)) = pending.pop() {
        let kinds = [
            ("subqueries", &scope.subquery_scopes),
            ("derived", &scope.derived_table_scopes),
            ("ctes", &scope.cte_scopes),
            ("branches", &scope.union_scopes),
            ("udtfs", &scope.udtf_scopes),
        ];
        // A derived table's scope keeps its query, which the source that names it holds.
        let derived = |child: &Scope| {
            scope.sources.values().find_map(|source| match &source.expression {
                Expression::Subquery(subquery) if subquery.this == child.expression => {
                    Name::of(subquery.alias.as_ref()?, sql, dialect)
                }
                _ => None,
            })
        };
        for (kind, children) in kinds {
            pending.extend(children.iter().enumerate().map(|(at, child)| {
                let owner = match (kind, &child.expression) {
                    ("ctes", Expression::Cte(cte)) => Name::of(&cte.alias, sql, dialect),
                    ("derived", _) => derived(child),
                    ("branches", _) => owner.clone(),
                    _ => None,
                };
                (format!("{path}.{kind}[{at}]"), child, owner)
            }));
        }
        found.push((path, scope, owner));
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
            names.push(name(expression, sql, dialect));
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

/// The name of a select list's `expression`, parsed from `sql`: its alias, or a column's
/// own name; none for any other expression, which the engine would name.
fn name(expression: &Expression, sql: &[char], dialect: Dialect) -> Option<Name> {
    match expression {
        Expression::Alias(alias) => Name::of(&alias.alias, sql, dialect),
        Expression::Column(column) => Name::of(&column.name, sql, dialect),
        _ => None,
    }
}
