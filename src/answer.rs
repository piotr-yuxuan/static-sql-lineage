// SPDX-License-Identifier: EUPL-1.2
//! What is said of one write: its output columns, its edges, its gaps and its refusal.

use polyglot_sql::expressions::Expression;
use polyglot_sql::traversal::ExpressionWalk;
use polyglot_sql::validation::{SchemaValidationOptions, validate_with_schema};
use polyglot_sql::{AnalyzeQueryOptions, QueryAnalysis, ValidationSeverity, analyze_query};
use serde::Serialize;

use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::edge::{Edge, End, Reach};
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
    /// A gap of `reason` that concerns no relation, and the output column `column`, if any.
    pub fn of(reason: GapReason, column: Option<String>, detail: impl Into<String>) -> Gap {
        Gap { reason, relation: None, column, detail: detail.into() }
    }
}

/// Why an edge may be missing; in the order of their names, by which gaps are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    AmbiguousRelation,
    AnalysisFailed,
    UnattributedUse,
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
                ..Gap::of(GapReason::UnknownRelation, None, "unknown to the catalog")
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
        let outputs = match write.outputs(&query, catalog, dialect, defaults) {
            Ok(Outputs::Named(outputs)) => outputs,
            Ok(Outputs::Unknown(relation)) => {
                gaps.push(Gap { relation, ..Gap::of(GapReason::Wildcard, None, "the columns of a * are unknown") });
                return answer.ended(gaps);
            }
            Err(refusal) => return answer.refused(refusal),
        };
        let answer = Answer { columns: Some(outputs.clone()), ..answer };
        let alike: Vec<Gap> = (known.iter().enumerate())
            .flat_map(|(at, one)| {
                known[at + 1..].iter().filter(|other| one.alike(other)).map(move |other| (one, other))
            })
            .map(|(one, other)| Gap::of(GapReason::AmbiguousRelation, None, format!("{one} and {other}")))
            .collect();
        if !alike.is_empty() {
            return answer.ended(gaps.into_iter().chain(alike).collect());
        }
        // polyglot gives a pivot's grouping column no use, and `UNPIVOT`'s name column a copy
        // of the columns it unpivots (#559, #560).
        if query.dfs().any(|node| matches!(node, Expression::Pivot(_) | Expression::Unpivot(_))) {
            gaps.push(Gap::of(GapReason::AnalysisFailed, None, "the engine does not trace PIVOT and UNPIVOT"));
            return answer.ended(gaps);
        }
        let options =
            AnalyzeQueryOptions { dialect: dialect.polyglot(), schema: Some(schema.clone()), complexity_guard: None };
        // The projection that names each output column: the one at its place, else the only
        // one that does, as the engine orders a `*` over a join as DuckDB does.
        let order = |analysis: &QueryAnalysis| -> Option<Vec<usize>> {
            let projections = &analysis.projections;
            let found = (outputs.iter().enumerate()).map(|(place, output)| {
                let named = |at: &usize| projections[*at].name.as_deref().is_some_and(|name| output.reported_as(name));
                let mut others = (0..projections.len()).filter(named);
                match (named(&place), others.next(), others.next()) {
                    (true, ..) => Some(place),
                    (false, Some(at), None) => Some(at),
                    _ => None,
                }
            });
            found.collect::<Option<Vec<usize>>>().filter(|_| projections.len() == outputs.len())
        };
        let analysis = match analyze_query(&write.sql, options) {
            Ok(analysis) => analysis,
            Err(error) => {
                gaps.push(Gap::of(GapReason::AnalysisFailed, None, error.to_string()));
                return answer.ended(gaps);
            }
        };
        let Some(order) = order(&analysis) else {
            gaps.push(Gap::of(GapReason::AnalysisFailed, None, "the analysis names other output columns"));
            return answer.ended(gaps);
        };
        // polyglot leaves an output column that a `*` as a function's argument feeds with no
        // upstream (#556); `count(*)` holds no `*`.
        let starred = |node: &Expression| matches!(node, Expression::Function(function) if function.args.iter().any(|arg| matches!(arg, Expression::Star(_))));
        if query.dfs().any(starred) {
            gaps.push(Gap::of(GapReason::Wildcard, None, "the engine does not trace a * as a function's argument"));
        }
        let children: Vec<End> =
            outputs.iter().map(|output| End { relation: write.target.clone(), column: output.to_string() }).collect();
        let holders = write.holders(&query, dialect);
        let keys = write.keys(&query, &outputs, catalog, dialect, defaults);
        let steps = Write::steps(&query, &holders, &keys, &order, &schema, dialect);
        let reach = Reach::of(&query, holders, keys, steps);
        let (edges, found) = Edge::of_analysis(&analysis, &order, &reach, &children, relations, catalog);
        gaps.extend(found);
        Answer { edges, ..answer }.ended(gaps)
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
