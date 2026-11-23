// SPDX-License-Identifier: EUPL-1.2
//! Edges: how a column of a relation that a write reads reaches a column of its target.

use polyglot_sql::expressions::{Expression, Select};
use polyglot_sql::scope::SourceKind;
use polyglot_sql::traversal::ExpressionWalk;
use polyglot_sql::{ColumnReferenceFact, ColumnUseContext, QueryAnalysis, ReferenceConfidence, TransformKind};
use serde::Serialize;

use crate::answer::{Gap, GapReason};
use crate::catalog::Catalog;
use crate::relation::RelationName;

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
}

/// One end of an edge: a relation and one of its columns, spelled as the catalog or the
/// write that made them known spells them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct End {
    pub relation: String,
    pub column: String,
}

/// `(parent relation, column) -> (child relation, column)`, with its kind.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
        let unresolved = || format!("{source}.{}", reference.column);
        let resolved =
            reference.confidence == ReferenceConfidence::Resolved && reference.source_kind == SourceKind::Table;
        let mut read = relations.iter().filter(|relation| resolved && relation.reported_as(source));
        let relation = read.next().ok_or_else(unresolved)?;
        if read.any(|other| other != relation) {
            return Err(unresolved());
        }
        let Some((known, columns)) = catalog.get(relation) else { return Ok(None) };
        let mut columns = columns.iter().filter(|column| column.reported_as(&reference.column));
        match (columns.next(), columns.next()) {
            (Some(column), None) => {
                let parent = End { relation: known.to_string(), column: column.to_string() };
                Ok(Some(Edge { parent, child, kind }))
            }
            _ => Err(unresolved()),
        }
    }

    /// The edges to `children`, the ends of a write's output columns in their order, and
    /// the gaps, from the engine's `analysis` of its query: each projection's upstream
    /// references by their kind, and each use's references as `rows` to the output
    /// columns that `reach` places it at; one edge per pair of columns, of the first kind.
    pub fn of_analysis(
        analysis: &QueryAnalysis,
        reach: &Reach,
        children: &[End],
        relations: &[RelationName],
        catalog: &Catalog,
    ) -> (Vec<Edge>, Vec<Gap>) {
        let mut gaps = Vec::new();
        let mut found: Vec<(usize, &ColumnReferenceFact, Kind)> = (analysis.projections.iter().enumerate())
            .flat_map(|(place, projection)| {
                let values = projection.upstream.iter();
                values.filter_map(move |reference| Some((place, reference, Kind::of(reference.path_transform_kind?)?)))
            })
            .collect();
        for use_ in &analysis.column_uses {
            let Some(places) = reach.rows(&use_.scope_path, use_.context) else {
                let detail = format!("{:?} at {}: {}", use_.context, use_.scope_path, use_.expression_sql);
                gaps.push(Gap::of(GapReason::UnattributedUse, None, detail));
                continue;
            };
            let used = places.into_iter().flat_map(|place| use_.references.iter().map(move |used| (place, used)));
            found.extend(used.map(|(place, used)| (place, &used.reference, Kind::Rows)));
        }
        let mut edges = Vec::new();
        for (place, reference, kind) in found {
            let child = &children[place];
            match Edge::of_reference(reference, kind, child.clone(), relations, catalog) {
                Ok(Some(edge)) => edges.push((place, edge)),
                Ok(None) => {}
                Err(detail) => gaps.push(Gap::of(GapReason::UnresolvedReference, Some(child.column.clone()), detail)),
            }
        }
        edges.sort();
        edges.dedup_by(|one, other| one.0 == other.0 && one.1.parent == other.1.parent);
        (edges.into_iter().map(|(_, edge)| edge).collect(), gaps)
    }
}

/// Where the clauses of one write's query reach, from its tree: whether a `SELECT` of it
/// keeps rows by their order, whether its outermost query does, which CTEs of its
/// outermost `WITH` that query reads, directly or through others of that `WITH`, and the
/// places of its output columns.
pub struct Reach {
    limited: bool,
    outermost: bool,
    read: Vec<usize>,
    every: Vec<usize>,
}

impl Reach {
    /// The reach of the clauses of `query`, which has `outputs` output columns. A CTE is
    /// matched by its name as the engine matches it, ignoring case.
    pub fn of(query: &Expression, outputs: usize) -> Reach {
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
        let named = |tree: &Expression| -> Vec<String> {
            let one_part = |node: &Expression| match node {
                Expression::Table(table) if table.schema.is_none() && table.catalog.is_none() => {
                    Some(table.name.name.to_ascii_lowercase())
                }
                _ => None,
            };
            tree.dfs().filter_map(one_part).collect()
        };
        // What the outermost query reads outside its CTEs' bodies, then what each CTE
        // that it reads reads.
        let mut pending = named(query);
        for name in ctes.iter().flat_map(|cte| named(&cte.this)) {
            if let Some(at) = pending.iter().position(|other| *other == name) {
                pending.swap_remove(at);
            }
        }
        let mut read = Vec::new();
        while let Some(name) = pending.pop() {
            let cte = ctes.iter().position(|cte| cte.alias.name.eq_ignore_ascii_case(&name));
            if let Some(at) = cte.filter(|at| !read.contains(at)) {
                read.push(at);
                pending.extend(named(&ctes[at].this));
            }
        }
        let limited = query.dfs().any(|node| matches!(node, Expression::Select(select) if limited(select)));
        Reach { limited: limited || outermost, outermost, read, every: (0..outputs).collect() }
    }

    /// The places of the output columns whose rows a use of `context` in the scope at
    /// `path` decides; none when where it reaches cannot be placed. A clause reaches
    /// every output column from the outermost query, its derived tables and its
    /// branches, and from a CTE of its outermost `WITH` that it reads; not from a
    /// subquery, a table function or another CTE.
    pub fn rows(&self, path: &str, context: ColumnUseContext) -> Option<Vec<usize>> {
        let steps = path.strip_prefix("root").and_then(|steps| {
            match steps.strip_prefix(".ctes[").and_then(|rest| rest.split_once(']')) {
                Some((at, rest)) => at.parse().ok().filter(|at| self.read.contains(at)).map(|_| rest),
                None => Some(steps),
            }
        });
        let placed = steps.is_some_and(|steps| {
            steps.split('.').skip(1).all(|step| step.starts_with("derived[") || step.starts_with("branches["))
        });
        match context {
            ColumnUseContext::Join
            | ColumnUseContext::Filter
            | ColumnUseContext::Group
            | ColumnUseContext::Having
            | ColumnUseContext::Qualify
            | ColumnUseContext::SetOperationFilter
            | ColumnUseContext::TableFunctionInput => placed.then(|| self.every.clone()),
            // The engine counts these in the upstream of the projection that holds them.
            ColumnUseContext::WindowPartition
            | ColumnUseContext::WindowOrder
            | ColumnUseContext::WindowFrame
            | ColumnUseContext::AggregateOrder => Some(Vec::new()),
            ColumnUseContext::Order if !self.limited => Some(Vec::new()),
            ColumnUseContext::Order => (self.outermost && path == "root").then(|| self.every.clone()),
        }
    }
}
