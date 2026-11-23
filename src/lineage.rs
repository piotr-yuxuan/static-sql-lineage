// SPDX-License-Identifier: EUPL-1.2
//! The lineage of the writes read from scripts: their answers in migration order, the edges
//! of all of them, and the walk over those edges from a column.

use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::answer::{Answer, Gap, Refusal, spelled};
use crate::catalog::Catalog;
use crate::dialect::Dialect;
use crate::edge::{Edge, End};
use crate::graph::{Graph, Search};
use crate::relation::{Name, RelationName, listing};
use crate::script::{Finding, Scripted};
use crate::write::Write;

/// The writes of scripts in migration order and their answers; the edges of all the
/// answers, one per pair of ends, of the first kind where several writes give it, each with
/// the ids of those writes; the scripts' refused writes and what they do not read; and the
/// catalog as the scripts' declarations give it.
pub struct Lineage<'w> {
    writes: &'w [Scripted],
    refused: Vec<&'w Finding>,
    not_read: Vec<&'w Finding>,
    pub answers: Vec<Answer>,
    edges: Vec<(Edge, Vec<String>)>,
    declared: Catalog,
    dialect: Dialect,
}

/// A walk over the lineage from a column, `from`: each edge that it follows, with the ids of
/// its writes and the depth of its first reaching; the gaps and the refusals of the writes
/// that it meets, each with its write's id; the scripts' refused writes and what they do not
/// read, such as dynamic SQL, any of which may read or write the columns that it follows;
/// and the cycles among those columns.
pub struct Walk<'l> {
    pub from: &'l End,
    pub edges: Vec<(&'l Edge, &'l [String], usize)>,
    pub gaps: Vec<(&'l str, &'l Gap)>,
    pub refusals: Vec<(&'l str, &'l Refusal)>,
    pub refused: Vec<&'l Finding>,
    pub not_read: Vec<&'l Finding>,
    pub cycles: Vec<Vec<&'l End>>,
}

impl<'w> Lineage<'w> {
    /// The lineage of the writes of `graph`, in migration order, each answered with its text
    /// and the defaults in force at its statement against `catalog`, as the scripts'
    /// declarations give it and the writes before it change it, under `dialect`.
    pub fn of(graph: &'w Graph, mut catalog: Catalog, dialect: Dialect) -> Lineage<'w> {
        let (writes, declared) = (graph.writes.as_slice(), catalog.clone());
        let refused = graph.findings.iter().filter(|finding| matches!(finding, Finding::Refused { write: true, .. }));
        let not_read = graph.findings.iter().filter(|finding| matches!(finding, Finding::NotRead { .. }));
        let (mut answers, mut edges, mut found) = (Vec::new(), Vec::new(), HashMap::new());
        for scripted in writes {
            let (id, target, sql) = (scripted.id.clone(), scripted.target.to_string(), scripted.text.clone());
            let (answer, replaces) =
                Answer::of(&Write { id, target, sql }, &scripted.target, &catalog, dialect, &scripted.defaults);
            catalog.record(scripted.target.clone(), replaces, answer.columns.clone());
            for edge in &answer.edges {
                let at = *found.entry((edge.parent.clone(), edge.child.clone())).or_insert(edges.len());
                if at == edges.len() {
                    edges.push((edge.clone(), Vec::new()));
                }
                let (known, ids): &mut (Edge, Vec<String>) = &mut edges[at];
                known.kind = known.kind.min(edge.kind);
                ids.push(scripted.id.clone());
            }
            answers.push(answer);
        }
        let (refused, not_read) = (refused.collect(), not_read.collect());
        Lineage { writes, refused, not_read, answers, edges, declared, dialect }
    }

    /// The walk from `from`, upstream, from children to parents, or downstream: each column
    /// once, breadth first, each edge at the depth of its first reaching. A write is met at a
    /// column of its target upstream, or of a relation that it reads downstream, and gives
    /// its refusal and its gaps on the whole write or on a column of its target that the walk
    /// follows. None when no declaration, edge's end or answer's output names `from`.
    pub fn walk<'l>(&'l self, from: &'l End, upstream: bool) -> Option<Walk<'l>> {
        let answered = self.writes.iter().zip(&self.answers);
        let output = |(write, answer): (&Scripted, &Answer)| {
            write.target == from.relation
                && answer.columns.as_ref().is_some_and(|columns| columns.contains(&from.column))
        };
        let declared = self.declared.get(&from.relation).is_some_and(|(_, columns)| columns.contains(&from.column));
        let ended = self.edges.iter().any(|(edge, _)| edge.parent == *from || edge.child == *from);
        if !(declared || ended || answered.clone().any(output)) {
            return None;
        }
        // Each edge's ends, the one that the walk comes from first.
        let ends = |edge: &'l Edge| if upstream { (&edge.child, &edge.parent) } else { (&edge.parent, &edge.child) };
        let mut next: HashMap<&End, Vec<usize>> = HashMap::new();
        for (at, (edge, _)) in self.edges.iter().enumerate() {
            next.entry(ends(edge).0).or_default().push(at);
        }
        // The columns in the order of their reaching, each with its depth, and the columns
        // that each reaches.
        let (mut columns, mut places, mut reaches, mut edges) =
            (vec![(from, 0)], HashMap::from([(from, 0)]), vec![Vec::new()], Vec::new());
        let mut at = 0;
        while let Some(&(column, depth)) = columns.get(at) {
            for &edge in next.get(column).into_iter().flatten() {
                let ((known, ids), reached) = (&self.edges[edge], ends(&self.edges[edge].0).1);
                let place = *places.entry(reached).or_insert(columns.len());
                if place == columns.len() {
                    columns.push((reached, depth + 1));
                    reaches.push(Vec::new());
                }
                reaches[at].push(place);
                edges.push((known, ids.as_slice(), depth + 1));
            }
            at += 1;
        }
        let (_, components) = Search::components(&reaches);
        let cycles =
            components.into_iter().filter(|members| members.len() > 1 || reaches[members[0]].contains(&members[0]));
        let cycles = cycles.map(|members| members.into_iter().map(|at| columns[at].0).collect()).collect();
        let relations: HashSet<&RelationName> = columns.iter().map(|(column, _)| &column.relation).collect();
        let met = |(write, _): &(&Scripted, &Answer)| {
            if upstream {
                relations.contains(&write.target)
            } else {
                write.reads.iter().any(|read| relations.contains(read))
            }
        };
        let (mut gaps, mut refusals) = (Vec::new(), Vec::new());
        for (write, answer) in answered.filter(met) {
            let followed = |column: &String| {
                let column = Name::read(column, self.dialect);
                column.is_ok_and(|column| places.contains_key(&End { relation: write.target.clone(), column }))
            };
            let given = answer.gaps.iter().filter(|gap| gap.column.as_ref().is_none_or(followed));
            gaps.extend(given.map(|gap| (write.id.as_str(), gap)));
            refusals.extend(answer.refusal.iter().map(|refusal| (write.id.as_str(), refusal)));
        }
        let (refused, not_read) = (self.refused.clone(), self.not_read.clone());
        Some(Walk { from, edges, gaps, refusals, refused, not_read, cycles })
    }
}

impl Walk<'_> {
    /// The walk as JSON, or as text: one line per edge, indented by two spaces for each depth
    /// after the first, with the ids of its writes; then one line per gap, per refusal, of the
    /// writes met then of the scripts, per finding not read, and per cycle, after its kind and
    /// a colon.
    pub fn printed(&self, json: bool) -> String {
        if json {
            let edges = self.edges.iter().map(|(edge, writes, depth)| {
                let (parent, child, kind) = (&edge.parent, &edge.child, edge.kind);
                json!({"parent": parent, "child": child, "kind": kind, "writes": writes, "depth": depth})
            });
            let gaps = self.gaps.iter().map(|(write, gap)| {
                let (reason, relation, column) = (gap.reason, &gap.relation, &gap.column);
                json!({"write": write, "reason": reason, "relation": relation, "column": column})
            });
            let refusals =
                self.refusals.iter().map(|(write, refusal)| json!({"write": write, "reason": refusal.reason}));
            // A finding of the scripts, by its place and its detail.
            let placed = |finding: &&Finding| match finding {
                Finding::Refused { place, detail, .. } | Finding::NotRead { place, detail } => {
                    Some(json!({"place": place, "detail": detail}))
                }
                _ => None,
            };
            let (edges, gaps, refusals): (Vec<_>, Vec<_>, Vec<_>) =
                (edges.collect(), gaps.collect(), refusals.chain(self.refused.iter().filter_map(placed)).collect());
            let not_read: Vec<_> = self.not_read.iter().filter_map(placed).collect();
            let (from, cycles) = (self.from, &self.cycles);
            return json!({
                "column": from, "edges": edges, "gaps": gaps, "refusals": refusals, "not_read": not_read, "cycles": cycles,
            })
            .to_string();
        }
        let edges = self
            .edges
            .iter()
            .map(|(edge, writes, depth)| format!("{}{edge} {}", "  ".repeat(depth - 1), writes.join(", ")));
        let gaps = self.gaps.iter().map(|(write, gap)| format!("gap: {write} {gap}"));
        let refusals =
            self.refusals.iter().map(|(write, refusal)| format!("refused: {write} {}", spelled(&refusal.reason)));
        let refused = self.refused.iter().chain(&self.not_read).map(ToString::to_string);
        let cycles = self.cycles.iter().map(|cycle| format!("cycle: {}", listing(cycle)));
        edges.chain(gaps).chain(refusals).chain(refused).chain(cycles).collect::<Vec<_>>().join("\n")
    }
}
