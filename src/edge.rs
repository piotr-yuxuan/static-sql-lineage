// SPDX-License-Identifier: EUPL-1.2
//! Edges: how a column of a relation that a write reads reaches a column of its target.

use std::fmt;

use polyglot_sql::expressions::JoinKind;
use polyglot_sql::scope::SourceKind;
use polyglot_sql::{ColumnReferenceFact, QueryAnalysis, ReferenceConfidence, TransformKind};
use serde::Serialize;

use crate::answer::{Gap, GapReason, spelled};
use crate::catalog::Catalog;
use crate::relation::{Name, RelationName};
use crate::write::Reach;

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

    /// The ends of `columns`, columns of `relation`, in their order.
    pub fn of_columns(relation: &RelationName, columns: &[Name]) -> Vec<End> {
        columns.iter().map(|column| End { relation: relation.clone(), column: column.clone() }).collect()
    }
}

/// `(parent relation, column) -> (child relation, column)`, with its kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub parent: End,
    pub child: End,
    pub kind: Kind,
}

impl fmt::Display for Edge {
    /// `<parent> -> <child> <kind>`.
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{} -> {} {}", self.parent, self.child, spelled(&self.kind))
    }
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
    /// the gaps, from the engine's `analysis` of its query, whose projections `reach`'s
    /// `order` gives for each: each projection's upstream references by their kind, as
    /// `reach` says they reach, and each use's references as `rows` to the output columns
    /// that `reach` places it at; one edge per pair of columns, of the first kind. No
    /// `rows` edge comes from `own`, a target whose own row the query reads (Q4).
    pub fn of_analysis(
        analysis: &QueryAnalysis,
        reach: &Reach,
        children: &[End],
        relations: &[RelationName],
        catalog: &Catalog,
        own: Option<&RelationName>,
    ) -> (Vec<Edge>, Vec<Gap>) {
        let mut gaps = Vec::new();
        // Each reference with its place and, for a projection's, its kind: a use's is rows.
        let mut found: Vec<(usize, &ColumnReferenceFact, Option<Kind>)> = (reach.order.iter().enumerate())
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
                Some(kind) => edges.push((Some(place), Edge { kind, ..edge })),
                None => {
                    let detail = "the engine's lineage does not say how its values reach it";
                    gaps.push(Gap::of(GapReason::UnattributedUse, Some(child.column.to_string()), detail));
                }
            }
        }
        (Edge::ordered(edges), gaps)
    }

    /// A write's `edges`, each with its child's place among the write's output columns,
    /// none first: ordered by that place, then by the parent's spelling and the child's,
    /// as protocol 1 orders edges, then by their kind; the first of each pair of ends.
    pub fn ordered(mut edges: Vec<(Option<usize>, Edge)>) -> Vec<Edge> {
        edges.sort_by_cached_key(|(place, Edge { parent, child, kind })| {
            (*place, parent.relation.to_string(), parent.column.to_string(), child.column.to_string(), *kind)
        });
        edges.dedup_by(|(_, one), (_, other)| one.child == other.child && one.parent == other.parent);
        edges.into_iter().map(|(_, edge)| edge).collect()
    }
}
