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
use crate::relation::{Name, RelationName};
use crate::write::{Outputs, Piece, Write};

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

    /// The gap of `relation`, which the catalog does not know.
    fn unknown(relation: &RelationName) -> Gap {
        Gap {
            relation: Some(relation.to_string()),
            ..Gap::of(GapReason::UnknownRelation, None, "unknown to the catalog")
        }
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
    OtherTarget,
    Invalid,
    UnnamedColumn,
    DuplicateColumn,
}

impl Answer {
    /// The answer to `write`, whose target is `target`, against `catalog` as it stands just
    /// before it, and whether the write replaces its target, as one may that does not
    /// parse.
    pub fn of(
        write: &Write,
        target: &RelationName,
        catalog: &Catalog,
        dialect: Dialect,
        defaults: &[Name],
    ) -> (Answer, bool) {
        let answer =
            Answer { write: write.id.clone(), columns: None, edges: Vec::new(), gaps: Vec::new(), refusal: None };
        let (found, replaces, pieces) = match write.pieces(target, catalog, dialect, defaults) {
            Ok(read) => read,
            Err(refusal) => {
                let replaces = matches!(refusal.reason, RefusalReason::Unparsed);
                return (answer.refused(refusal), replaces);
            }
        };
        let mut answers = pieces.iter().map(|piece| Answer::of_piece(write, &found, piece, catalog, dialect, defaults));
        if replaces {
            return (answers.next().expect("a replace write is one piece"), true);
        }
        let (mut edges, mut gaps) = (Vec::new(), Vec::new());
        for piece in answers {
            if piece.refusal.is_some() {
                return (piece, false);
            }
            edges.extend(piece.edges);
            gaps.extend(piece.gaps);
        }
        // A contribute write's columns are its target's that its pieces map onto, in the
        // target's order, or, the target unknown, a gap for it and those that the statement
        // names; its edges are ordered by them, one per pair of columns, of the first kind.
        let known = catalog.get(target).map(|(_, columns)| columns);
        gaps.extend(known.is_none().then(|| Gap::unknown(&found)));
        let mapped = pieces.iter().flat_map(|piece| piece.columns.iter().flatten());
        let columns: Vec<Name> = match known {
            Some(known) => {
                known.iter().filter(|column| mapped.clone().any(|other| other == *column)).cloned().collect()
            }
            None => mapped.cloned().collect(),
        };
        let ends: Vec<End> =
            columns.iter().map(|column| End { relation: write.target.clone(), column: column.to_string() }).collect();
        let place = |edge: &Edge| ends.iter().position(|end| *end == edge.child);
        edges.sort_by(|one, other| (place(one), one).cmp(&(place(other), other)));
        edges.dedup_by(|one, other| one.child == other.child && one.parent == other.parent);
        (Answer { columns: known.map(|_| columns), edges, ..answer }.ended(gaps), false)
    }

    /// The answer to `piece`, one of `write`'s, whose target the statement names `target`,
    /// against `catalog`: its output columns, or its target's columns onto which they map,
    /// its edges and its gaps, or its refusal.
    pub fn of_piece(
        write: &Write,
        target: &RelationName,
        piece: &Piece,
        catalog: &Catalog,
        dialect: Dialect,
        defaults: &[Name],
    ) -> Answer {
        let answer =
            Answer { write: write.id.clone(), columns: None, edges: Vec::new(), gaps: Vec::new(), refusal: None };
        let sql = &piece.text;
        let query = match Write::query(sql, dialect) {
            Ok(query) => query,
            Err(refusal) => return answer.refused(refusal),
        };
        let relations = &Write::relations(sql, &query, dialect, defaults);
        let (known, unknown): (Vec<_>, Vec<_>) = relations.iter().partition(|relation| catalog.get(relation).is_some());
        let mut gaps: Vec<Gap> = unknown.into_iter().map(Gap::unknown).collect();
        let schema = catalog.schema(relations);
        let text = dialect.engine_text(sql);
        let checked = validate_with_schema(&text, dialect.polyglot(), &schema, &SchemaValidationOptions::default());
        let errors: Vec<String> = (checked.errors.into_iter())
            .filter(|error| error.severity == ValidationSeverity::Error && error.code != "E200")
            .map(|error| error.message)
            .collect();
        if !errors.is_empty() {
            return answer.refused(Refusal { reason: RefusalReason::Invalid, detail: errors.join("; ") });
        }
        let outputs = match Write::outputs(sql, &query, piece.columns.as_deref(), catalog, dialect, defaults) {
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
        // one that does, as the engine orders a `*` over a join as DuckDB does; for a piece
        // whose outputs map by position, the one at its place.
        let order = |analysis: &QueryAnalysis| -> Option<Vec<usize>> {
            let projections = &analysis.projections;
            let found = (outputs.iter().enumerate()).map(|(place, output)| {
                let named = |at: &usize| projections[*at].name.as_deref().is_some_and(|name| output.reported_as(name));
                let mut others = (0..projections.len()).filter(named);
                match (piece.columns.is_some() || named(&place), others.next(), others.next()) {
                    (true, ..) => Some(place),
                    (false, Some(at), None) => Some(at),
                    _ => None,
                }
            });
            found.collect::<Option<Vec<usize>>>().filter(|_| projections.len() == outputs.len())
        };
        let analysis = match analyze_query(&text, options) {
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
        let holders = Write::holders(sql, &query, dialect);
        let keys = Write::keys(sql, &query, catalog, dialect, defaults);
        let steps = Write::steps(&text, &query, &holders, keys.as_deref(), &order, &schema, dialect);
        let reach = Reach::of(sql, &query, outputs.len(), holders, keys, steps, dialect);
        let own = piece.own.then_some(target);
        let (edges, found) = Edge::of_analysis(&analysis, &order, &reach, &children, relations, catalog, own);
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
