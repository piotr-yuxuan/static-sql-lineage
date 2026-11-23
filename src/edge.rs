// SPDX-License-Identifier: EUPL-1.2
//! Edges: how a column of a relation that a write reads reaches a column of its target.

use polyglot_sql::expressions::{Expression, Select};
use polyglot_sql::scope::SourceKind;
use polyglot_sql::traversal::ExpressionWalk;
use polyglot_sql::{ColumnReferenceFact, ColumnUseContext, ColumnUseFact, ReferenceConfidence, TransformKind};
use serde::Serialize;

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
}

/// Where the clauses of one write's query reach, from its tree: whether a `SELECT` of it
/// keeps rows by their order, whether its outermost query does, and which CTEs of its
/// outermost `WITH` that query reads, directly or through others of that `WITH`.
pub struct Reach {
    limited: bool,
    outermost: bool,
    read: Vec<usize>,
}

impl Reach {
    /// The reach of the clauses of `query`. A CTE is matched by its name as the engine
    /// matches it, ignoring case.
    pub fn of(query: &Expression) -> Reach {
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
        Reach { limited: limited || outermost, outermost, read }
    }

    /// Whether `use_` decides the rows of every output column; none when where it
    /// reaches cannot be placed. A clause reaches every output column from the outermost
    /// query, its derived tables and its branches, and from a CTE of its outermost
    /// `WITH` that it reads; not from a subquery, a table function or another CTE.
    pub fn rows(&self, use_: &ColumnUseFact) -> Option<bool> {
        let path = use_.scope_path.strip_prefix("root").and_then(|path| {
            match path.strip_prefix(".ctes[").and_then(|rest| rest.split_once(']')) {
                Some((at, rest)) => at.parse().ok().filter(|at| self.read.contains(at)).map(|_| rest),
                None => Some(path),
            }
        });
        let placed = path.is_some_and(|path| {
            path.split('.').skip(1).all(|step| step.starts_with("derived[") || step.starts_with("branches["))
        });
        match use_.context {
            ColumnUseContext::Join
            | ColumnUseContext::Filter
            | ColumnUseContext::Group
            | ColumnUseContext::Having
            | ColumnUseContext::Qualify
            | ColumnUseContext::SetOperationFilter
            | ColumnUseContext::TableFunctionInput => placed.then_some(true),
            // The engine counts these in the upstream of the projection that holds them.
            ColumnUseContext::WindowPartition
            | ColumnUseContext::WindowOrder
            | ColumnUseContext::WindowFrame
            | ColumnUseContext::AggregateOrder => Some(false),
            ColumnUseContext::Order if !self.limited => Some(false),
            ColumnUseContext::Order => (self.outermost && use_.scope_path == "root").then_some(true),
        }
    }
}
