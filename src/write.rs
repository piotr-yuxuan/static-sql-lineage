// SPDX-License-Identifier: EUPL-1.2
//! A write as the request gives it: the pieces that its statement is read into, and the
//! query of each, the relations it reads, and the names of its output columns.

use std::collections::HashSet;

use polyglot_sql::expressions::{
    AlterViewAction, Cte, Expression, From, Identifier, Join, JoinKind, Literal, Merge, Null, Select, TableRef,
    ViewColumn, Where, With,
};
use polyglot_sql::scope::{Scope, SourceKind, build_scope, walk_in_scope};
use polyglot_sql::traversal::{ExpressionWalk, get_merge_target};
use polyglot_sql::validation::{ValidationSchema, mapping_schema_from_validation_schema_with_dialect};
use polyglot_sql::{ColumnUseContext, LineageNode, TokenType, generate, lineage_at_with_schema, parse_one};
use serde::Deserialize;

use crate::answer::{Refusal, RefusalReason};
use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::relation::{Name, RelationName, repeated};

/// The request's form of a write: its id, its target and its SQL, a query or a statement.
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

/// A key that a join merges in a `SELECT` of a write's query: the CTE or derived table
/// whose select it is, none for the outermost; whether the engine's lineage names that
/// select's step, which it does not for a branch of a set operation or a subquery; the
/// column that its list names, the key that the column reads, and whether an expression
/// around the key computes the column; the kind of the join; and the relation or CTE that
/// the join joins, when it joins one.
pub struct Key {
    pub owner: Option<Name>,
    pub placed: bool,
    pub column: Name,
    pub read: Name,
    pub computed: bool,
    pub join: JoinKind,
    pub right: Option<RelationName>,
}

/// One `SELECT` that a write is read into, as text; the target's columns onto which its
/// output columns map by position, or none where its query names them; and whether it
/// reads the target's own row, whose conditions then give no edge.
pub struct Piece {
    pub text: String,
    pub columns: Option<Vec<Name>>,
    pub own: bool,
}

impl Write {
    /// The pieces that the write's statement is read into, its target as the statement
    /// names it, and whether it replaces its target. A query is one piece, its own text,
    /// and writes `target`, or the table of its `INTO`; a view, or a table made by
    /// `CREATE TABLE ... AS`, is its query, onto its column list, if any. An `INSERT` is
    /// its query; an `UPDATE`, the `SELECT` of its values over its sources, its target among
    /// them; a `DELETE`, of a `NULL` for every column of the target; a `MERGE`, one for its
    /// `ON` as a `DELETE`, then one for each `WHEN` clause. A contribute statement's pieces map onto the target's columns, as `catalog`
    /// knows them, or, the target unknown, onto those that the statement names; a piece
    /// that would need the target's columns is left out. Refused when the statement does not parse, is not a
    /// write that the tool reads, names another target than `target`, which completes it as
    /// it completes the request's names, or a column that the target lacks.
    pub fn pieces(
        &self,
        target: &RelationName,
        catalog: &Catalog,
        dialect: Dialect,
        defaults: &[Name],
    ) -> Result<(RelationName, bool, Vec<Piece>), Refusal> {
        let refused = |reason, detail: String| Refusal { reason, detail };
        let statement = (parse_one(&self.sql, dialect.polyglot()))
            .map_err(|error| refused(RefusalReason::Unparsed, error.to_string()))?;
        let sql: Vec<char> = self.sql.chars().collect();
        let known = catalog.get(target).map(|(_, columns)| columns);
        // The values of a piece that decides the rows of every column of the target.
        let nulls = vec![Expression::Null(Null); known.map_or(0, <[Name]>::len)];
        let not_a_write = || refused(RefusalReason::NotAQuery, "not a write that the tool reads".to_string());
        // The statement's target, none for a query's; whether it replaces it; whether its
        // pieces read its own row; each piece, its tree or none for the statement's own
        // text, with its columns.
        let (table, replaces, own, specs) = match &statement {
            Expression::CreateView(view) => {
                let columns = view_columns(&view.columns, &view.name, &self.sql, dialect);
                (Some(&view.name), true, false, vec![(Some(view.query.clone()), columns)])
            }
            Expression::AlterView(view) => {
                let query = (view.actions.iter())
                    .find_map(|action| match action {
                        AlterViewAction::AsSelect(query) => Some(query),
                        _ => None,
                    })
                    .ok_or_else(not_a_write)?;
                let columns = view_columns(&view.columns, &view.name, &self.sql, dialect);
                (Some(&view.name), true, false, vec![(Some((**query).clone()), columns)])
            }
            Expression::CreateTable(create) => {
                let query = with(create.as_select.clone().ok_or_else(not_a_write)?, create.with_cte.clone());
                let columns = create.columns.iter().map(|column| Name::of(&column.name, &sql, dialect));
                let columns = columns.collect::<Option<Vec<Name>>>().filter(|columns| !columns.is_empty());
                (Some(&create.name), true, false, vec![(Some(query), Ok(columns))])
            }
            Expression::Insert(insert) => {
                let query = insert.query.clone().filter(|_| !insert.by_name).ok_or_else(not_a_write)?;
                let columns = columns(&insert.columns, known, &sql, dialect);
                (Some(&insert.table), false, false, vec![(Some(with(query, insert.with.clone())), columns)])
            }
            Expression::Update(update) => {
                // T-SQL's `UPDATE a ... FROM t AS a` names as its target the source that `a`
                // names; any other target is added to the sources.
                let known_as = |table: &TableRef| Name::of(table.alias.as_ref().unwrap_or(&table.name), &sql, dialect);
                let froms = update.from_clause.iter().flat_map(|from| &from.expressions);
                let named = froms.clone().find_map(|source| match source {
                    Expression::Table(table) if known_as(table) == known_as(&update.table) => Some(&**table),
                    _ => None,
                });
                let added = named.is_none().then(|| Expression::Table(Box::new(update.table.clone())));
                // polyglot gives every target as an expression, in `assignments`, once any is
                // qualified.
                let assignments =
                    update.assignments.iter().map(|pair| Some((assigned(&pair.left)?, pair.right.clone())));
                let pairs = update.set.iter().map(|(column, value)| Some((column, value.clone()))).chain(assignments);
                let pairs = pairs.collect::<Option<Vec<_>>>().ok_or_else(not_a_write)?;
                let (set, values): (Vec<&Identifier>, Vec<Expression>) = pairs.into_iter().unzip();
                let condition = update.where_clause.as_ref().map(|filter| filter.this.clone());
                let sources = added.into_iter().chain(froms.cloned()).collect();
                let piece = synthetic(values, sources, update.from_joins.clone(), condition, update.with.clone());
                let columns = columns(set, known, &sql, dialect);
                (Some(named.unwrap_or(&update.table)), false, true, vec![(Some(piece), columns)])
            }
            Expression::Delete(delete) => {
                let alias = delete.alias.clone().or_else(|| delete.table.alias.clone());
                let sources =
                    std::iter::once(TableRef { alias, ..delete.table.clone() }).chain(delete.using.iter().cloned());
                let sources = sources.map(|source| Expression::Table(Box::new(source))).collect();
                let condition = delete.where_clause.as_ref().map(|filter| filter.this.clone());
                let piece = synthetic(nulls, sources, delete.joins.clone(), condition, delete.with.clone());
                (Some(&delete.table), false, true, vec![(Some(piece), Ok(known.map(<[Name]>::to_vec)))])
            }
            Expression::Merge(merge) => {
                let Some(Expression::Table(table)) = get_merge_target(&statement) else { return Err(not_a_write()) };
                let with = merge.with_.as_deref().and_then(|with| match with {
                    Expression::With(with) => Some((**with).clone()),
                    _ => None,
                });
                let sources = vec![(*merge.this).clone(), (*merge.using).clone()];
                let whens = whens(merge, &sources, with.as_ref(), &nulls).ok_or_else(not_a_write)?.into_iter();
                let whens = whens.map(|(piece, named)| (Some(piece), columns(&named, known, &sql, dialect)));
                let on = synthetic(nulls, sources, Vec::new(), merge.on.as_deref().cloned(), with);
                let specs = std::iter::once((Some(on), Ok(known.map(<[Name]>::to_vec)))).chain(whens);
                (Some(&**table), false, true, specs.collect())
            }
            // `SELECT ... INTO` names its target in the `SELECT`, or in the left branch of a
            // set operation, and is traced as the query that it is.
            query @ (Expression::Select(_)
            | Expression::Union(_)
            | Expression::Intersect(_)
            | Expression::Except(_)
            | Expression::Subquery(_)) => {
                let into = query.dfs().find_map(|node| match node {
                    Expression::Select(select) => select.into.as_ref(),
                    _ => None,
                });
                let into = into.map(|into| match &into.this {
                    Expression::Table(table) => Ok(&**table),
                    _ => Err(not_a_write()),
                });
                (into.transpose()?, true, false, vec![(None, Ok(None))])
            }
            _ => return Err(not_a_write()),
        };
        let found = match table {
            None => target.clone(),
            Some(table) => (RelationName::of(table, &sql, dialect, defaults).filter(|found| found == target))
                .ok_or_else(|| refused(RefusalReason::OtherTarget, format!("the statement does not write {target}")))?,
        };
        let mut pieces = Vec::new();
        for (tree, columns) in specs {
            let columns = columns?;
            if !replaces && columns.is_none() {
                continue;
            }
            let text = tree.map_or_else(|| Ok(self.sql.clone()), |tree| generate(&tree, dialect.polyglot()));
            let text = text.map_err(|error| refused(RefusalReason::Unparsed, error.to_string()))?;
            pieces.push(Piece { text, columns, own });
        }
        Ok((found, replaces, pieces))
    }

    /// The query that `sql` holds, the outermost one when it is parenthesised, as the
    /// engine reads it; refused when it is not one statement, or not a query.
    pub fn query(sql: &str, dialect: Dialect) -> Result<Expression, Refusal> {
        let parsed = parse_one(sql, dialect.polyglot())
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

    /// The relations that `query`, parsed from `sql`, reads: its table references that
    /// name no CTE in scope, as the engine scopes them and as the dialect compares names,
    /// and those that Snowflake's `IDENTIFIER` names by its one string literal in a `FROM`
    /// or a join, ordered by their spelling.
    pub fn relations(sql: &str, query: &Expression, dialect: Dialect, defaults: &[Name]) -> Vec<RelationName> {
        let sql: Vec<char> = sql.chars().collect();
        let root = build_scope(query);
        // The engine's scopes match a CTE's name ignoring ASCII case alone.
        let cte = |scope: &Scope, table: &TableRef| {
            table.schema.is_none()
                && scope.cte_sources.values().any(|source| {
                    matches!(&source.expression, Expression::Cte(cte) if Name::same(&cte.alias, &table.name, &sql, dialect))
                })
        };
        let scoped = (scopes(&root, &sql, dialect).into_iter())
            .flat_map(|(_, scope, _)| scope.sources.values().map(move |source| (scope, source)));
        let mut relations: Vec<RelationName> =
            (scoped.filter_map(|(scope, source)| match (&source.kind, &source.expression) {
                (SourceKind::Table, Expression::Table(table)) if !cte(scope, table) => {
                    RelationName::of(table, &sql, dialect, defaults)
                }
                _ => None,
            }))
            .collect();
        // The engine scopes `IDENTIFIER('<name>')` as a table that the SQL does not spell.
        let selects = query.dfs().filter_map(|node| match node {
            Expression::Select(select) => Some(select),
            _ => None,
        });
        relations.extend(selects.flat_map(|select| sources(select)).filter_map(|(_, source)| {
            let Expression::Function(function) = (match source {
                Expression::Alias(alias) => &alias.this,
                source => source,
            }) else {
                return None;
            };
            let [Expression::Literal(literal)] = function.args.as_slice() else { return None };
            let Literal::String(text) = literal.as_ref() else { return None };
            function
                .name
                .eq_ignore_ascii_case("identifier")
                .then(|| RelationName::read(text, dialect, defaults).ok())?
        }));
        relations.sort_by_key(ToString::to_string);
        relations
    }

    /// The names of the output columns of `query`, parsed from `sql`, as [`named`] gives
    /// them, or `columns` onto which they map by position; refused when they are not as
    /// many as `columns`, or when one is unnamed, or two are named alike.
    pub fn outputs(
        sql: &str,
        query: &Expression,
        columns: Option<&[Name]>,
        catalog: &Catalog,
        dialect: Dialect,
        defaults: &[Name],
    ) -> Result<Outputs, Refusal> {
        let sql: Vec<char> = sql.chars().collect();
        let names = match named(query, &[], &sql, catalog, dialect, defaults) {
            Ok(names) => names,
            Err(relation) => return Ok(Outputs::Unknown(relation)),
        };
        let names = match columns {
            Some(columns) if columns.len() == names.len() => columns.iter().cloned().map(Some).collect(),
            Some(columns) => {
                let detail = format!("{} output columns for {} columns", names.len(), columns.len());
                return Err(Refusal { reason: RefusalReason::Invalid, detail });
            }
            None => names,
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

    /// Each subquery of `query`, parsed from `sql`, that no subquery holds, by the scope
    /// path that the engine gives it, with its holder; none for one that the engine numbers
    /// otherwise, or that follows a `*` in its select list, where its place would count the
    /// `*` as one. The engine numbers the subqueries of a `SELECT` by its
    /// `collect_subqueries`: the fields that hold them, in the order of its struct, each
    /// walked by `walk_in_scope`, finding a `Subquery` with no alias, the query of `EXISTS`
    /// and of `IN`, and that of `ANY` and `ALL` twice, as a quantifier's and as a
    /// `Subquery`.
    pub fn holders(sql: &str, query: &Expression, dialect: Dialect) -> Vec<(String, Option<Holder>)> {
        let sql: Vec<char> = sql.chars().collect();
        let root = build_scope(query);
        let all = scopes(&root, &sql, dialect);
        // A branch that filters an `INTERSECT` or an `EXCEPT`, or a branch of one, decides
        // rows by its select list.
        let filters = |path: &str| {
            let filtered = |scope: &Scope| matches!(query_of(scope), Expression::Intersect(_) | Expression::Except(_));
            let set = |at: usize| all.iter().any(|(parent, scope, _)| *parent == path[..at] && filtered(scope));
            path.match_indices(".branches[1]").any(|(at, _)| set(at))
        };
        let mut found = Vec::new();
        for (path, scope, owner) in &all {
            let Expression::Select(select) = query_of(scope) else { continue };
            if path.contains(".subqueries[") || path.contains(".udtfs[") {
                continue;
            }
            // In the select list, the output column at its place or, in a CTE or a
            // derived table, the column that the list names there.
            let clause = |context| Some(Holder::Clause(context));
            let filtering = filters(path);
            let star = select_list(select).position(|expression| matches!(expression, Expression::Star(_)));
            let listed = select_list(select).enumerate().map(|(at, expression)| {
                if filtering {
                    return (clause(ColumnUseContext::SetOperationFilter), expression);
                }
                let holder = owner.as_ref().map_or(Some(Holder::Output(at)), |owner| {
                    Some(Holder::Column(owner.clone(), name(expression, &sql, dialect)?))
                });
                (holder.filter(|_| star.is_none_or(|star| at < star)), expression)
            });
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

    /// The keys that the joins of `query`, parsed from `sql`, merge in each of its
    /// `SELECT`s, selected bare, aliased or not, through a `*`, or read bare by an
    /// expression of its select list; none when a join merges a key among sources whose
    /// columns are unknown, where its keys are not told.
    pub fn keys(
        sql: &str,
        query: &Expression,
        catalog: &Catalog,
        dialect: Dialect,
        defaults: &[Name],
    ) -> Option<Vec<Key>> {
        let sql: Vec<char> = sql.chars().collect();
        let root = build_scope(query);
        let ctes: Vec<&Cte> = (root.cte_scopes.iter())
            .filter_map(|cte| match &cte.expression {
                Expression::Cte(cte) => Some(&**cte),
                _ => None,
            })
            .collect();
        let bare = |expression: &Expression| match expression {
            Expression::Column(column) if column.table.is_none() => Name::of(&column.name, &sql, dialect),
            _ => None,
        };
        let right = |join: &Join| match &join.this {
            Expression::Table(table) => RelationName::of(table, &sql, dialect, defaults),
            _ => None,
        };
        let mut keys = Vec::new();
        for (path, scope, owner) in scopes(&root, &sql, dialect) {
            let Expression::Select(select) = query_of(scope) else { continue };
            if !select.joins.iter().any(|join| !join.using.is_empty() || natural(join)) {
                continue;
            }
            let placed = !["subqueries[", "udtfs[", "branches["].iter().any(|kind| path.contains(kind));
            let columns = joined(select, &ctes, &sql, catalog, dialect, defaults).ok()?;
            // Each column of the list with each column of the sources that it reads bare, and
            // whether an expression around it computes the column.
            let listed = select_list(select).flat_map(|expression| match expression {
                Expression::Star(star) if star.table.is_none() => {
                    columns.iter().map(|(column, _)| (column.clone(), column.clone(), false)).collect()
                }
                Expression::Alias(alias) => {
                    let named = name(expression, &sql, dialect);
                    let computed = bare(&alias.this).is_none();
                    let read = walk_in_scope(&alias.this, false).filter_map(bare);
                    read.filter_map(|read| Some((named.clone()?, read, computed))).collect()
                }
                column => name(column, &sql, dialect)
                    .zip(bare(column))
                    .map(|(named, read)| (named, read, false))
                    .into_iter()
                    .collect::<Vec<_>>(),
            });
            for (named, read, computed) in listed {
                let merged = columns.iter().filter(|(column, _)| *column == read).flat_map(|(_, joins)| joins);
                keys.extend(merged.map(|&at| {
                    let (join, right) = (select.joins[at].kind, right(&select.joins[at]));
                    Key {
                        owner: owner.clone(),
                        placed,
                        column: named.clone(),
                        read: read.clone(),
                        computed,
                        join,
                        right,
                    }
                }));
            }
        }
        Some(keys)
    }

    /// The engine's lineage of each output column of `query`, the engine's projection
    /// that `order` gives for it, against `schema`, read from `text`, the text that the
    /// engine reads for `query`; none where the engine gives none; for a write that holds
    /// an `INTERSECT` or an `EXCEPT`, a key that a join merges, as `keys` says, or a
    /// subquery in a CTE's or a derived table's select list, as `holders` says, and for no
    /// other.
    pub fn steps(
        text: &str,
        query: &Expression,
        holders: &[(String, Option<Holder>)],
        keys: Option<&[Key]>,
        order: &[usize],
        schema: &ValidationSchema,
        dialect: Dialect,
    ) -> Vec<Option<LineageNode>> {
        let filtering = query.dfs().any(|node| matches!(node, Expression::Intersect(_) | Expression::Except(_)));
        let held = holders.iter().any(|(_, holder)| matches!(holder, Some(Holder::Column(..))));
        if !(filtering || held || keys.is_some_and(|keys| !keys.is_empty())) {
            return Vec::new();
        }
        let schema = mapping_schema_from_validation_schema_with_dialect(schema, dialect.polyglot());
        let read = Write::query(text, dialect).ok();
        let lineage =
            |place| lineage_at_with_schema(place, read.as_ref()?, Some(&schema), Some(dialect.polyglot()), false).ok();
        order.iter().map(|&at| lineage(at)).collect()
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
    for (at, source) in sources(select) {
        let right = columns_of(source)?;
        let Some(at) = at else {
            columns.extend(right.into_iter().map(|column| (column, Vec::new())));
            continue;
        };
        let join = &select.joins[at];
        let using: Vec<Name> = join.using.iter().filter_map(|key| Name::of(key, sql, dialect)).collect();
        let merges = |column: &Name| using.contains(column) || natural(join) && right.contains(column);
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
                    .rposition(|cte| table.schema.is_none() && Name::same(&cte.alias, &table.name, sql, dialect));
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
    let mut names = Vec::new();
    for expression in select_list(select) {
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
                let called = |(_, source): &(Option<usize>, &Expression)| match source {
                    Expression::Table(table) => {
                        Name::same(table.alias.as_ref().unwrap_or(&table.name), qualifier, sql, dialect)
                    }
                    Expression::Subquery(subquery) => {
                        subquery.alias.as_ref().is_some_and(|alias| Name::same(alias, qualifier, sql, dialect))
                    }
                    _ => false,
                };
                let (_, source) = sources(select).find(called).ok_or(None)?;
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

/// The sources of `select`: those of its `FROM`, then the table of each join, with the
/// join's place.
fn sources(select: &Select) -> impl Iterator<Item = (Option<usize>, &Expression)> {
    let froms = select.from.iter().flat_map(|from| &from.expressions).map(|source| (None, source));
    froms.chain(select.joins.iter().enumerate().map(|(at, join)| (Some(at), &join.this)))
}

/// The query of `scope`, that of its CTE for a CTE's.
fn query_of(scope: &Scope) -> &Expression {
    match &scope.expression {
        Expression::Cte(cte) => &cte.this,
        query => query,
    }
}

/// Whether `join` is `NATURAL`, which merges the columns that both its sides have.
fn natural(join: &Join) -> bool {
    matches!(join.kind, JoinKind::Natural | JoinKind::NaturalLeft | JoinKind::NaturalRight | JoinKind::NaturalFull)
}

/// The expressions of the select list of `select`, each out of the `Annotated` that the
/// engine wraps around a column after a comment.
fn select_list(select: &Select) -> impl Iterator<Item = &Expression> {
    select.expressions.iter().map(|expression| match expression {
        Expression::Annotated(annotated) => &annotated.this,
        expression => expression,
    })
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

/// The target's columns that `identifiers`, parsed from `sql`, name, as `known`, the
/// target's columns, spells them, or as `sql` does when the target is unknown; or, for no
/// identifier, every column of the target, unknown with it. Refused when one does not
/// read as a name, is named twice, or the target lacks one.
fn columns<'a>(
    identifiers: impl IntoIterator<Item = &'a Identifier>,
    known: Option<&[Name]>,
    sql: &[char],
    dialect: Dialect,
) -> Result<Option<Vec<Name>>, Refusal> {
    let refused = |detail: String| Refusal { reason: RefusalReason::Invalid, detail };
    let named = identifiers.into_iter().map(|identifier| Name::of(identifier, sql, dialect));
    let unread = || refused("a column of its list does not read as a name".to_string());
    let named = named.collect::<Option<Vec<Name>>>().ok_or_else(unread)?;
    if let Some(name) = repeated(&named) {
        return Err(refused(format!("the statement names {name} twice")));
    }
    if named.is_empty() {
        return Ok(known.map(<[Name]>::to_vec));
    }
    let Some(known) = known else { return Ok(Some(named)) };
    let found = named.iter().map(|name| {
        let column = known.iter().find(|column| *column == name).cloned();
        column.ok_or_else(|| refused(format!("the target has no column {name}")))
    });
    found.collect::<Result<_, _>>().map(Some)
}

/// The names of `columns`, the column list of the view `view`, read from the tokens of
/// `sql` that follow its name, since the parser keeps neither their spans nor their
/// quotes: the token that opens each item of the list; none for no list.
fn view_columns(
    columns: &[ViewColumn],
    view: &TableRef,
    sql: &str,
    dialect: Dialect,
) -> Result<Option<Vec<Name>>, Refusal> {
    let refused = |detail: String| Refusal { reason: RefusalReason::Invalid, detail };
    if columns.is_empty() {
        return Ok(None);
    }
    let tokens = polyglot_sql::Dialect::get(dialect.polyglot()).tokenize(sql);
    let tokens = tokens.map_err(|error| refused(error.to_string()))?;
    let chars: Vec<char> = sql.chars().collect();
    let after = view.name.span.map_or(0, |span| span.end);
    let (mut depth, mut opens, mut names) = (0, false, Vec::new());
    for token in tokens.iter().filter(|token| token.span.start >= after) {
        match token.token_type {
            TokenType::LParen => depth += 1,
            TokenType::RParen if depth == 1 => break,
            TokenType::RParen => depth -= 1,
            _ if opens => {
                let quoted = token.token_type == TokenType::QuotedIdentifier;
                let span = Some(token.span);
                let identifier = Identifier { name: token.text.clone(), quoted, trailing_comments: Vec::new(), span };
                names.push(Name::of(&identifier, &chars, dialect).ok_or_else(|| refused(token.text.clone()))?);
            }
            _ => {}
        }
        opens = depth == 1 && matches!(token.token_type, TokenType::LParen | TokenType::Comma);
    }
    Ok(Some(names))
}

/// The `SELECT` of each `WHEN` clause of `merge`, its values over `sources`, under `with`,
/// where its condition holds, with the columns that its `UPDATE SET` or its `INSERT` names;
/// none for an `INSERT` without a list, whose values follow the target's columns, or for a
/// `DELETE`, whose values are `nulls`, one for each column of the target. None when a
/// clause names a target that is not a column.
fn whens(
    merge: &Merge,
    sources: &[Expression],
    with: Option<&With>,
    nulls: &[Expression],
) -> Option<Vec<(Expression, Vec<Identifier>)>> {
    let clauses = merge.whens.iter().flat_map(|whens| match whens.as_ref() {
        Expression::Whens(whens) => whens.expressions.as_slice(),
        _ => &[],
    });
    let mut pieces = Vec::new();
    for clause in clauses {
        let Expression::When(when) = clause else { continue };
        // polyglot gives an action as a tuple of its keyword and its lists, a `DELETE` as its
        // keyword alone.
        let (values, named): (Vec<Expression>, Vec<Identifier>) = match when.then.as_ref() {
            Expression::Tuple(action) => match action.expressions.as_slice() {
                [Expression::Var(keyword), Expression::Tuple(set)] if keyword.this.eq_ignore_ascii_case("update") => {
                    let pairs = set.expressions.iter().map(|pair| match pair {
                        Expression::Eq(pair) => Some((pair.right.clone(), assigned(&pair.left)?.clone())),
                        _ => None,
                    });
                    pairs.collect::<Option<Vec<_>>>()?.into_iter().unzip()
                }
                [_, Expression::Tuple(named), Expression::Tuple(values)] => {
                    let named = named.expressions.iter().map(|item| assigned(item).cloned());
                    (values.expressions.clone(), named.collect::<Option<_>>()?)
                }
                [_, Expression::Tuple(values)] => (values.expressions.clone(), Vec::new()),
                _ => continue,
            },
            _ => (nulls.to_vec(), Vec::new()),
        };
        let piece = synthetic(values, sources.to_vec(), Vec::new(), when.condition.as_deref().cloned(), with.cloned());
        pieces.push((piece, named));
    }
    Some(pieces)
}

/// The column that `expression`, the target of an assignment or an item of a column list,
/// names, by its last part; none for any other expression, such as a tuple.
fn assigned(expression: &Expression) -> Option<&Identifier> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier),
        Expression::Column(column) => Some(&column.name),
        _ => None,
    }
}

/// The synthetic `SELECT` of a piece of an `UPDATE`, a `DELETE` or a `MERGE`: `values`
/// over `sources` and `joins`, where `condition` holds, under the statement's `with`.
fn synthetic(
    values: Vec<Expression>,
    sources: Vec<Expression>,
    joins: Vec<Join>,
    condition: Option<Expression>,
    with: Option<With>,
) -> Expression {
    let mut select = Select::new();
    (select.expressions, select.joins, select.with) = (values, joins, with);
    select.from = Some(From { expressions: sources });
    select.where_clause = condition.map(|this| Where { this });
    Expression::Select(Box::new(select))
}

/// `query` under `with`, the `WITH` that its statement writes before it, unless it has
/// its own.
fn with(mut query: Expression, with: Option<With>) -> Expression {
    match &mut query {
        Expression::Select(select) => select.with = select.with.take().or(with),
        Expression::Union(set) => set.with = set.with.take().or(with),
        Expression::Intersect(set) => set.with = set.with.take().or(with),
        Expression::Except(set) => set.with = set.with.take().or(with),
        _ => {}
    }
    query
}
