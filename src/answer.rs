// SPDX-License-Identifier: EUPL-1.2
//! What is said of one write: its output columns, its edges, its gaps and its refusal.

use polyglot_sql::validation::{SchemaValidationOptions, validate_with_schema};
use polyglot_sql::{AnalyzeQueryOptions, ValidationSeverity, analyze_query};
use serde::Serialize;

use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::edge::{Edge, End, Kind};
use crate::relation::Name;
use crate::write::{Outputs, Write};

/// The answer to one write.
#[derive(Serialize)]
pub struct Answer {
    write: String,
    /// The output columns, when they are all known and the write is not refused.
    pub columns: Option<Vec<Name>>,
    edges: Vec<Edge>,
    gaps: Vec<Gap>,
    refusal: Option<Refusal>,
}

/// A place where an edge may be missing, and why.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Gap {
    reason: GapReason,
    /// The relation it concerns, as the write's SQL spells it.
    relation: Option<String>,
    /// The output column it concerns.
    column: Option<String>,
    detail: String,
}

impl Gap {
    /// A gap of `reason` that concerns no relation and no column.
    fn of(reason: GapReason, detail: impl Into<String>) -> Gap {
        Gap { reason, relation: None, column: None, detail: detail.into() }
    }
}

/// Why an edge may be missing; in the order of their names, by which gaps are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    AmbiguousRelation,
    AnalysisFailed,
    UnknownRelation,
    UnresolvedReference,
    Wildcard,
}

/// Why nothing of a write is traced.
#[derive(Debug, Serialize)]
pub struct Refusal {
    pub reason: RefusalReason,
    pub detail: String,
}

/// The reasons of a refusal.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalReason {
    Unparsed,
    NotAQuery,
    Invalid,
    UnnamedColumn,
    DuplicateColumn,
}

impl Answer {
    /// The answer to `write`, against `catalog` as it stands just before it.
    pub fn of(write: &Write, catalog: &Catalog, dialect: Dialect, defaults: &[Name]) -> Answer {
        let answer =
            Answer { write: write.id.clone(), columns: None, edges: Vec::new(), gaps: Vec::new(), refusal: None };
        let query = match write.query(dialect) {
            Ok(query) => query,
            Err(refusal) => return answer.refused(refusal),
        };
        let relations = &write.relations(&query, dialect, defaults);
        let (known, unknown): (Vec<_>, Vec<_>) = relations.iter().partition(|relation| catalog.get(relation).is_some());
        let mut gaps: Vec<Gap> = (unknown.into_iter())
            .map(|relation| Gap {
                relation: Some(relation.to_string()),
                ..Gap::of(GapReason::UnknownRelation, "unknown to the catalog")
            })
            .collect();
        let schema = catalog.schema(relations);
        let checked =
            validate_with_schema(&write.sql, dialect.polyglot(), &schema, &SchemaValidationOptions::default());
        let errors: Vec<String> = (checked.errors.into_iter())
            .filter(|error| error.severity == ValidationSeverity::Error && error.code != "E200")
            .map(|error| error.message)
            .collect();
        if !errors.is_empty() {
            return answer.refused(Refusal { reason: RefusalReason::Invalid, detail: errors.join("; ") });
        }
        let outputs = match write.outputs(&query, dialect) {
            Ok(Outputs::Named(outputs)) => outputs,
            Ok(Outputs::Unknown(relation)) => {
                gaps.push(Gap { relation, ..Gap::of(GapReason::Wildcard, "the columns of a * are unknown") });
                return answer.ended(gaps);
            }
            Err(refusal) => return answer.refused(refusal),
        };
        let answer = Answer { columns: Some(outputs.clone()), ..answer };
        let alike: Vec<Gap> = (known.iter().enumerate())
            .flat_map(|(at, one)| {
                known[at + 1..].iter().filter(|other| one.alike(other)).map(move |other| (one, other))
            })
            .map(|(one, other)| Gap::of(GapReason::AmbiguousRelation, format!("{one} and {other}")))
            .collect();
        if !alike.is_empty() {
            return answer.ended(gaps.into_iter().chain(alike).collect());
        }
        let options = AnalyzeQueryOptions { dialect: dialect.polyglot(), schema: Some(schema), complexity_guard: None };
        let analysis = match analyze_query(&write.sql, options) {
            Ok(analysis)
                if analysis.projections.len() == outputs.len()
                    && (analysis.projections.iter().zip(&outputs)).all(|(projection, output)| {
                        projection.name.as_deref().is_some_and(|name| output.reported_as(name))
                    }) =>
            {
                analysis
            }
            failed => {
                let detail = failed
                    .err()
                    .map_or("the analysis names other output columns".to_string(), |error| error.to_string());
                gaps.push(Gap::of(GapReason::AnalysisFailed, detail));
                return answer.ended(gaps);
            }
        };
        let found =
            (analysis.projections.iter().zip(&outputs)).enumerate().flat_map(|(place, (projection, output))| {
                let child = End { relation: write.target.clone(), column: output.to_string() };
                let values = projection
                    .upstream
                    .iter()
                    .filter_map(|reference| Some((reference, Kind::of(reference.path_transform_kind?)?)));
                values.map(move |(reference, kind)| {
                    (place, output, Edge::of_reference(reference, kind, child.clone(), relations, catalog))
                })
            });
        let mut edges = Vec::new();
        for (place, output, found) in found {
            match found {
                Ok(Some(edge)) => edges.push((place, edge)),
                Ok(None) => {}
                Err(detail) => gaps
                    .push(Gap { column: Some(output.to_string()), ..Gap::of(GapReason::UnresolvedReference, detail) }),
            }
        }
        edges.sort();
        edges.dedup_by(|one, other| one.0 == other.0 && one.1.parent == other.1.parent);
        Answer { edges: edges.into_iter().map(|(_, edge)| edge).collect(), ..answer }.ended(gaps)
    }

    /// This answer refused, with nothing traced.
    fn refused(self, refusal: Refusal) -> Answer {
        Answer { columns: None, edges: Vec::new(), gaps: Vec::new(), refusal: Some(refusal), ..self }
    }

    /// This answer with `gaps`, in their order, each once.
    fn ended(mut self, mut gaps: Vec<Gap>) -> Answer {
        gaps.sort();
        gaps.dedup();
        self.gaps = gaps;
        self
    }
}
