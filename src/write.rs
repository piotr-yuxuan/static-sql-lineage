// SPDX-License-Identifier: EUPL-1.2
//! A write as the request gives it: its query, the relations it reads, and the names of
//! its output columns.

use std::collections::HashSet;

use polyglot_sql::expressions::{Cte, Expression, Join, JoinKind, Select};
use polyglot_sql::scope::{Scope, SourceKind, build_scope, walk_in_scope};
use polyglot_sql::traversal::ExpressionWalk;
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

    /// For each of the output columns that `outputs` names, the joins of the outermost
    /// `SELECT` of `query` that merge it, each by its kind and the relation or CTE that it
    /// joins: a key that its `USING` names, or that its `NATURAL` finds on both its sides,
    /// selected bare, aliased or not, or through a `*`.
    pub fn keys(
        &self,
        query: &Expression,
        outputs: &[Name],
        catalog: &Catalog,
        dialect: Dialect,
        defaults: &[Name],
    ) -> Vec<Vec<(JoinKind, Option<RelationName>)>> {
        let sql: Vec<char> = self.sql.chars().collect();
        let Expression::Select(select) = query else { return vec![Vec::new(); outputs.len()] };
        let ctes: Vec<&Cte> = select.with.iter().flat_map(|with| &with.ctes).collect();
        let Ok(columns) = joined(select, &ctes, &sql, catalog, dialect, defaults) else {
            return vec![Vec::new(); outputs.len()];
        };
        let starred = (select.expressions.iter())
            .any(|expression| matches!(expression, Expression::Star(star) if star.table.is_none()));
        let bare = |expression: &Expression| match expression {
            Expression::Column(column) if column.table.is_none() => Name::of(&column.name, &sql, dialect),
            _ => None,
        };
        let joins = |join: &Join| match &join.this {
            Expression::Table(table) => RelationName::of(table, &sql, dialect, defaults),
            _ => None,
        };
        (outputs.iter())
            .map(|output| {
                let named = select
                    .expressions
                    .iter()
                    .find(|expression| name(expression, &sql, dialect).as_ref() == Some(output));
                let key = match named {
                    Some(Expression::Alias(alias)) => bare(&alias.this),
                    Some(column) => bare(column),
                    None => starred.then(|| output.clone()),
                };
                let merged = columns.iter().find(|(column, _)| key.as_ref() == Some(column));
                let merged = merged.into_iter().flat_map(|(_, joins)| joins.iter().map(|&at| &select.joins[at]));
                merged.map(|join| (join.kind, joins(join))).collect()
            })
            .collect()
    }

    /// The engine's lineage of each output column of `query`, the engine's projection
    /// that `order` gives for it, against `schema`, none where the engine gives none; for a
    /// write that holds an `INTERSECT` or an `EXCEPT`, a key that `keys` says a join
    /// merges, or a subquery in a CTE's or a derived table's select list, as `holders`
    /// says, and for no other.
    pub fn steps(
        query: &Expression,
        holders: &[(String, Option<Holder>)],
        keys: &[Vec<(JoinKind, Option<RelationName>)>],
        order: &[usize],
        schema: &ValidationSchema,
        dialect: Dialect,
    ) -> Vec<Option<LineageNode>> {
        let filtering = query.dfs().any(|node| matches!(node, Expression::Intersect(_) | Expression::Except(_)));
        let held = holders.iter().any(|(_, holder)| matches!(holder, Some(Holder::Column(..))));
        if !(filtering || held || keys.iter().any(|joins| !joins.is_empty())) {
            return Vec::new();
        }
        let schema = mapping_schema_from_validation_schema_with_dialect(schema, dialect.polyglot());
        let lineage = |place| lineage_at_with_schema(place, query, Some(&schema), Some(dialect.polyglot()), false);
        order.iter().map(|&at| lineage(at).ok()).collect()
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

/// The columns of the sources of `select`, as [`named`] gives them, each with the places
/// of the joins that merge it: a key that a join's `USING` names, or that its `NATURAL`
/// finds on both its sides, once. Snowflake puts a join's keys first, in the order of its
/// `USING` or, for `NATURAL`, of its left side, then its left side's other columns, then
/// its right side's; DuckDB keeps each key at its first place, and T-SQL merges none.
fn joined<'a>(
    select: &'a Select,
    ctes: &[&'a Cte],
    sql: &[char],
    catalog: &Catalog,
    dialect: Dialect,
    defaults: &[Name],
) -> Result<Vec<(Name, Vec<usize>)>, Option<String>> {
    let columns_of = |source| {
        let columns = named(source, ctes, sql, catalog, dialect, defaults)?;
        columns.into_iter().collect::<Option<Vec<Name>>>().ok_or(None)
    };
    let mut columns = Vec::new();
    for source in select.from.iter().flat_map(|from| &from.expressions) {
        columns.extend(columns_of(source)?.into_iter().map(|column| (column, Vec::new())));
    }
    for (at, join) in select.joins.iter().enumerate() {
        let right = columns_of(&join.this)?;
        let natural = matches!(
            join.kind,
            JoinKind::Natural | JoinKind::NaturalLeft | JoinKind::NaturalRight | JoinKind::NaturalFull
        );
        let using: Vec<Name> = join.using.iter().filter_map(|key| Name::of(key, sql, dialect)).collect();
        let merges = |column: &Name| using.contains(column) || natural && right.contains(column);
        for (column, joins) in &mut columns {
            if merges(column) {
                joins.push(at);
            }
        }
        let merged: Vec<&Name> =
            columns.iter().filter(|(_, joins)| joins.contains(&at)).map(|(column, _)| column).collect();
        let rest: Vec<Name> = right.iter().filter(|column| !merged.contains(column)).cloned().collect();
        columns.extend(rest.into_iter().map(|column| (column, Vec::new())));
        if dialect == Dialect::Snowflake {
            columns.sort_by_key(|(column, joins)| (!joins.contains(&at), using.iter().position(|key| key == column)));
        }
    }
    Ok(columns)
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
/// column, and a `*` gives those of the one source that its qualifier names, as [`named`]
/// gives them, or, unqualified, those of all its sources, as [`joined`] joins them, less
/// those that it excludes, as it renames them. Unknown when a `*` reaches an unnamed
/// column.
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
        let mut columns = match &star.table {
            None => joined(select, ctes, sql, catalog, dialect, defaults)?
                .into_iter()
                .map(|(column, _)| Some(column))
                .collect(),
            Some(qualifier) => {
                let called = |source: &&Expression| match source {
                    Expression::Table(table) => {
                        table.alias.as_ref().unwrap_or(&table.name).name.eq_ignore_ascii_case(&qualifier.name)
                    }
                    Expression::Subquery(subquery) => {
                        subquery.alias.as_ref().is_some_and(|alias| alias.name.eq_ignore_ascii_case(&qualifier.name))
                    }
                    _ => false,
                };
                let source = sources.iter().copied().find(called).ok_or(None)?;
                named(source, ctes, sql, catalog, dialect, defaults)?
            }
        };
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
