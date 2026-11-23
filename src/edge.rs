// SPDX-License-Identifier: EUPL-1.2
//! Edges: how a column of a relation that a write reads reaches a column of its target.

use polyglot_sql::scope::SourceKind;
use polyglot_sql::{ColumnReferenceFact, ReferenceConfidence, TransformKind};
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
