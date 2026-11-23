// SPDX-License-Identifier: EUPL-1.2
//! Edges: how a column of a relation that a write reads reaches a column of its target.

use std::fmt;

use polyglot_sql::expressions::{Expression, Identifier, JoinKind, Select};
use polyglot_sql::scope::SourceKind;
use polyglot_sql::traversal::ExpressionWalk;
use polyglot_sql::{
    ColumnReferenceFact, ColumnUseContext, LineageNode, QueryAnalysis, ReferenceConfidence, SetOperator, TransformKind,
};
use serde::Serialize;

use crate::answer::{Gap, GapReason};
use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::relation::{Name, RelationName};
use crate::write::{Holder, Key};

/// How a parent column reaches a child column, in the order in which one is kept when
/// several apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Copy,
    Derive,
    Rows,
}

impl Kind {
    /// The kind of a projection's upstream reference, by the strongest transform on its
    /// path; none for a constant.
    pub fn of(transform: TransformKind) -> Option<Kind> {
        match transform {
            TransformKind::Direct | TransformKind::Star => Some(Kind::Copy),
            TransformKind::Cast | TransformKind::Expression | TransformKind::Aggregation => Some(Kind::Derive),
            TransformKind::Constant => None,
        }
    }

    /// The kinds of the left and right sides of a key that a join of kind `join` merges:
    /// an inner join copies both; an outer one copies the side that it keeps, and the
    /// other decides its rows; a full one derives from both, as `coalesce` does.
    pub fn sides(join: JoinKind) -> Option<(Kind, Kind)> {
        match join {
            JoinKind::Inner | JoinKind::Natural => Some((Kind::Copy, Kind::Copy)),
            JoinKind::Left | JoinKind::NaturalLeft => Some((Kind::Copy, Kind::Rows)),
            JoinKind::Right | JoinKind::NaturalRight => Some((Kind::Rows, Kind::Copy)),
            JoinKind::Full | JoinKind::NaturalFull => Some((Kind::Derive, Kind::Derive)),
            _ => None,
        }
    }
}

/// One end of an edge: a relation and one of its columns, compared as names, spelled as
/// the catalog or the write that made them known spells them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct End {
    pub relation: RelationName,
    pub column: Name,
}

impl fmt::Display for End {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}.{}", self.relation, self.column)
    }
}

impl End {
    /// The column, reported as `column`, of the one known relation among `relations` that
    /// the engine reports as `source`: none for a relation unknown to the catalog, whose
    /// gap says it; the engine's text when it names no such column, or several.
    pub fn of(
        source: &str,
        column: &str,
        relations: &[RelationName],
        catalog: &Catalog,
    ) -> Result<Option<End>, String> {
        let unresolved = || format!("{source}.{column}");
        let mut read = relations.iter().filter(|relation| relation.reported_as(source));
        let relation = read.next().ok_or_else(unresolved)?;
        if read.any(|other| other != relation) {
            return Err(unresolved());
        }
        let Some((known, columns)) = catalog.get(relation) else { return Ok(None) };
        let mut columns = columns.iter().filter(|known| known.reported_as(column));
        match (columns.next(), columns.next()) {
            (Some(column), None) => Ok(Some(End { relation: known.clone(), column: column.clone() })),
            _ => Err(unresolved()),
        }
    }
}

/// `(parent relation, column) -> (child relation, column)`, with its kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub parent: End,
    pub child: End,
    pub kind: Kind,
}

impl Edge {
    /// The edge of `kind` from what `reference` reads to `child`: the column, reported as
    /// the reference's, of the one known relation among `relations` reported as its
    /// source. None for a relation unknown to the catalog, whose gap says it; the
    /// engine's text for a reference that resolves to no such column, or several.
    pub fn of_reference(
        reference: &ColumnReferenceFact,
        kind: Kind,
        child: End,
        relations: &[RelationName],
        catalog: &Catalog,
    ) -> Result<Option<Edge>, String> {
        let source = reference.source_name.as_deref().unwrap_or_default();
        if reference.confidence != ReferenceConfidence::Resolved || reference.source_kind != SourceKind::Table {
            return Err(format!("{source}.{}", reference.column));
        }
        Ok(End::of(source, &reference.column, relations, catalog)?.map(|parent| Edge { parent, child, kind }))
    }

    /// The edges to `children`, the ends of a write's output columns in their order, and
    /// the gaps, from the engine's `analysis` of its query, whose projections `order`
    /// gives for each: each projection's upstream references by their kind, as `reach`
    /// says they reach, and each use's references as `rows` to the output columns that
    /// `reach` places it at; one edge per pair of columns, of the first kind. No `rows`
    /// edge comes from `own`, a target whose own row the query reads (Q4).
    pub fn of_analysis(
        analysis: &QueryAnalysis,
        order: &[usize],
        reach: &Reach,
        children: &[End],
        relations: &[RelationName],
        catalog: &Catalog,
        own: Option<&RelationName>,
    ) -> (Vec<Edge>, Vec<Gap>) {
        let mut gaps = Vec::new();
        // Each reference with its place and, for a projection's, its kind: a use's is rows.
        let mut found: Vec<(usize, &ColumnReferenceFact, Option<Kind>)> = (order.iter().enumerate())
            .flat_map(|(place, &at)| {
                let values = analysis.projections[at].upstream.iter();
                values.filter_map(move |reference| {
                    Some((place, reference, Some(Kind::of(reference.path_transform_kind?)?)))
                })
            })
            .collect();
        for use_ in &analysis.column_uses {
            let Some(places) = reach.rows(&use_.scope_path, use_.context) else {
                let detail = format!("{:?} at {}: {}", use_.context, use_.scope_path, use_.expression_sql);
                gaps.push(Gap::of(GapReason::UnattributedUse, None, detail));
                continue;
            };
            let used = places.into_iter().flat_map(|place| use_.references.iter().map(move |used| (place, used)));
            found.extend(used.map(|(place, used)| (place, &used.reference, None)));
        }
        let mut edges = Vec::new();
        for (place, reference, value) in found {
            let child = &children[place];
            let edge = match Edge::of_reference(reference, Kind::Rows, child.clone(), relations, catalog) {
                Ok(Some(edge)) => edge,
                Ok(None) => continue,
                Err(detail) => {
                    gaps.push(Gap::of(GapReason::UnresolvedReference, Some(child.column.to_string()), detail));
                    continue;
                }
            };
            let source = reference.source_name.as_deref().unwrap_or_default();
            match value.map_or(Some(Kind::Rows), |kind| reach.kind(place, &edge.parent, kind, relations, catalog)) {
                Some(Kind::Rows) if own.is_some_and(|own| own.reported_as(source)) => {}
                Some(kind) => edges.push((place, Edge { kind, ..edge })),
                None => {
                    let detail = "the engine's lineage does not say how its values reach it";
                    gaps.push(Gap::of(GapReason::UnattributedUse, Some(child.column.to_string()), detail));
                }
            }
        }
        // By the child's place, then by the parent's spelling, as protocol 1 orders edges.
        edges.sort_by_cached_key(|(place, edge)| {
            (*place, edge.parent.relation.to_string(), edge.parent.column.to_string(), edge.kind)
        });
        edges.dedup_by(|one, other| one.0 == other.0 && one.1.parent == other.1.parent);
        (edges.into_iter().map(|(_, edge)| edge).collect(), gaps)
    }
}

/// Where the clauses of one write's query reach, from its tree: whether a `SELECT` of it
/// keeps rows by their order, whether its outermost query does, which CTEs of its
/// outermost `WITH` that query reads, directly or through others of that `WITH`, the
/// places of its output columns, the holders of its subqueries, the keys that its joins
/// merge, if their sides are told, and the engine's lineage of each output column, where
/// the write needs it.
pub struct Reach {
    limited: bool,
    outermost: bool,
    read: Vec<usize>,
    every: Vec<usize>,
    holders: Vec<(String, Option<Holder>)>,
    keys: Option<Vec<Key>>,
    steps: Vec<Option<LineageNode>>,
}

impl Reach {
    /// The reach of the clauses of `query`, parsed from `sql`, which has `outputs` output
    /// columns, its subqueries held as `holders` says, the keys that its joins merge
    /// `keys`, and the lineage of its output columns `steps`, or none. A CTE is matched by
    /// its name under the dialect.
    pub fn of<'q>(
        sql: &str,
        query: &'q Expression,
        outputs: usize,
        holders: Vec<(String, Option<Holder>)>,
        keys: Option<Vec<Key>>,
        steps: Vec<Option<LineageNode>>,
        dialect: Dialect,
    ) -> Reach {
        let sql: Vec<char> = sql.chars().collect();
        let limited = |select: &Select| {
            select.limit.is_some() || select.offset.is_some() || select.fetch.is_some() || select.top.is_some()
        };
        let (with, outermost) = match query {
            Expression::Select(select) => (select.with.as_ref(), limited(select)),
            Expression::Union(set) => (set.with.as_ref(), set.limit.is_some() || set.offset.is_some()),
            Expression::Intersect(set) => (set.with.as_ref(), set.limit.is_some() || set.offset.is_some()),
            Expression::Except(set) => (set.with.as_ref(), set.limit.is_some() || set.offset.is_some()),
            _ => (None, false),
        };
        let ctes = with.map_or(&[][..], |with| with.ctes.as_slice());
        let named = |tree: &'q Expression| -> Vec<&'q Identifier> {
            let one_part = |node: &'q Expression| match node {
                Expression::Table(table) if table.schema.is_none() && table.catalog.is_none() => Some(&table.name),
                _ => None,
            };
            tree.dfs().filter_map(one_part).collect()
        };
        // What the outermost query reads outside its CTEs' bodies, then what each CTE
        // that it reads reads.
        let mut pending = named(query);
        for name in ctes.iter().flat_map(|cte| named(&cte.this)) {
            if let Some(at) = pending.iter().position(|other| Name::same(other, name, &sql, dialect)) {
                pending.swap_remove(at);
            }
        }
        let mut read = Vec::new();
        while let Some(name) = pending.pop() {
            let cte = ctes.iter().position(|cte| Name::same(&cte.alias, name, &sql, dialect));
            if let Some(at) = cte.filter(|at| !read.contains(at)) {
                read.push(at);
                pending.extend(named(&ctes[at].this));
            }
        }
        let limited = query.dfs().any(|node| matches!(node, Expression::Select(select) if limited(select)));
        let every = (0..outputs).collect();
        Reach { limited: limited || outermost, outermost, read, every, holders, keys, steps }
    }

    /// The places of the output columns whose rows a use of `context` in the scope at
    /// `path` decides; none when where it reaches cannot be placed. A clause reaches
    /// every output column from the outermost query, its derived tables and its
    /// branches, and from a CTE of its outermost `WITH` that it reads; not from a table
    /// function or another CTE, nor when a tuple's `IN` in it leaves out columns that it
    /// reads. A use in a subquery reaches what the subquery's holder does.
    pub fn rows(&self, path: &str, context: ColumnUseContext) -> Option<Vec<usize>> {
        let tuple = |(subquery, holder): &(String, Option<Holder>)| {
            subquery.rsplit_once('.').is_some_and(|(parent, _)| parent == path)
                && matches!(holder, Some(Holder::Tuple(tuple)) if *tuple == context)
        };
        let steps = path.strip_prefix("root").and_then(|steps| {
            match steps.strip_prefix(".ctes[").and_then(|rest| rest.split_once(']')) {
                Some((at, rest)) => at.parse().ok().filter(|at| self.read.contains(at)).map(|_| rest),
                None => Some(steps),
            }
        });
        let placed = !self.holders.iter().any(tuple)
            && steps.is_some_and(|steps| {
                steps.split('.').skip(1).all(|step| step.starts_with("derived[") || step.starts_with("branches["))
            });
        // What a subquery at the path `subquery`, in the scope at `parent`, reaches: what a
        // use of its clause there does; the output column at its place in the select list
        // of the outermost query or a branch; the output columns whose lineage passes
        // through its column in the select list of a CTE or a derived table.
        let held = |parent: &str, subquery: &str| -> Option<Vec<usize>> {
            let (_, holder) = self.holders.iter().find(|(path, _)| path == subquery)?;
            match holder.as_ref()? {
                Holder::Clause(context) => self.rows(parent, *context),
                Holder::Tuple(_) => None,
                Holder::Output(place) => Some(vec![*place]),
                Holder::Column(source, column) => (self.every.iter())
                    .map(|&place| {
                        let mut nodes = self.steps.get(place)?.as_ref()?.walk();
                        Some(nodes.any(|node| source.reported_as(&node.source_name) && column.reported_as(&node.name)))
                            .map(|passes| passes.then_some(place))
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(|places| places.into_iter().flatten().collect()),
            }
        };
        match context {
            // The engine counts these in the upstream of the projection that holds them.
            ColumnUseContext::WindowPartition
            | ColumnUseContext::WindowOrder
            | ColumnUseContext::WindowFrame
            | ColumnUseContext::AggregateOrder => Some(Vec::new()),
            ColumnUseContext::Order if !self.limited => Some(Vec::new()),
            ColumnUseContext::Order => (self.outermost && path == "root").then(|| self.every.clone()),
            _ => match path.find(".subqueries[") {
                Some(at) => held(&path[..at], &path[..at + path[at..].find(']')? + 1]),
                None => placed.then(|| self.every.clone()),
            },
        }
    }

    /// The kind with which a projection's reference of `kind`, from `parent`, reaches the
    /// output column at `place`, by the engine's lineage of that column where the write
    /// needs it: the weaker of its own and that of its path, rows through the filtering
    /// branch of an `INTERSECT` or an `EXCEPT`. For a key that a join merges, in the
    /// outermost `SELECT` or the CTE or derived table whose step the lineage names, the
    /// path gives the kind of the side that it comes through, and its steps' own
    /// expressions stand for the reference's kind, which the engine takes from the merge's
    /// `COALESCE`. None where the lineage does not say, such as for a key merged across
    /// more than two sources, or whose right side the lineage does not tell from its left.
    pub fn kind(
        &self,
        place: usize,
        parent: &End,
        kind: Kind,
        relations: &[RelationName],
        catalog: &Catalog,
    ) -> Option<Kind> {
        let keys = self.keys.as_ref()?;
        let Some(step) = self.steps.get(place) else { return Some(kind) };
        let step = step.as_ref()?;
        let leaf = |node: &LineageNode| match (&node.source_kind, &node.expression) {
            (SourceKind::Table, Expression::Column(column)) => {
                End::of(&node.source_name, &column.name.name, relations, catalog).ok().flatten()
            }
            _ => None,
        };
        // A step copies when its own expression is a column, aliased or not.
        let expressed = |node: &LineageNode| {
            let mut expression = &node.expression;
            loop {
                expression = match expression {
                    Expression::Alias(alias) => &alias.this,
                    Expression::Paren(paren) => &paren.this,
                    Expression::Annotated(annotated) => &annotated.this,
                    Expression::Column(_) => return Kind::Copy,
                    _ => return Kind::Derive,
                };
            }
        };
        // The kinds of the left and right sides of the key merged into the column of
        // `node`, with the right's relation or CTE: the outermost `SELECT`'s, at the root,
        // or those of the CTE or derived table that its step names; none for no key, and
        // none at all where the lineage does not tell the sides, nor which step is the
        // merge of a key in a branch or a subquery, which any step of its name may be.
        // The column that a step stands for: a leaf's, or that of a CTE or derived table.
        let stands = |next: &LineageNode| match &next.expression {
            Expression::Column(column) => column.name.name.clone(),
            _ => next.name.clone(),
        };
        let sides = |node: &LineageNode, root: bool| {
            let step = matches!(node.source_kind, SourceKind::Cte | SourceKind::DerivedTable);
            let owns = |key: &&Key| match &key.owner {
                _ if !key.placed => true,
                None => root,
                Some(owner) => step && owner.reported_as(&node.source_name),
            };
            let merged: Vec<&Key> = keys.iter().filter(owns).filter(|key| key.column.reported_as(&node.name)).collect();
            let names = |key: &Key, right: &RelationName| {
                let sides = node.downstream.iter().filter(|next| key.read.reported_as(&stands(next)));
                sides.filter(|next| right.reported_as(&next.source_name)).count()
            };
            match merged.as_slice() {
                [] => Some(None),
                [key @ Key { placed: true, right: Some(right), .. }] if names(key, right) == 1 => {
                    Some(Some((Kind::sides(key.join)?, right, *key)))
                }
                _ => None,
            }
        };
        // Each step with the kind that its path gives, if any, the kind of the steps' own
        // expressions down to it, and whether the path crosses a merge.
        let mut pending = vec![(step, None, Kind::Copy, false, true)];
        let mut reached = Vec::new();
        while let Some((node, through, own, crossed, root)) = pending.pop() {
            let filtering =
                node.set_branch.filter(|branch| branch.ordinal == 1 && branch.operator != SetOperator::Union);
            let through = through.max(filtering.map(|_| Kind::Rows));
            let sides = sides(node, root)?;
            // A merge's own expression holds its `COALESCE`, which its sides' kinds replace;
            // an expression around the key derives.
            let own = sides
                .map_or(own.max(expressed(node)), |(_, _, key)| if key.computed { own.max(Kind::Derive) } else { own });
            if leaf(node).as_ref() == Some(parent) {
                let reference = if crossed { own } else { kind };
                reached.push(through.map_or(reference, |through| through.max(reference)));
            }
            // A merge's sides are the steps that stand for its key.
            pending.extend(node.downstream.iter().map(|next| {
                let side = sides
                    .filter(|(_, _, key)| key.read.reported_as(&stands(next)))
                    .map(|((left, right), joined, _)| if joined.reported_as(&next.source_name) { right } else { left });
                (next, through.max(side), own, crossed || side.is_some(), false)
            }));
        }
        Some(reached.into_iter().min().unwrap_or(kind))
    }
}
