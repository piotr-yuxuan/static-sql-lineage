// SPDX-License-Identifier: EUPL-1.2
//! The graph of the writes read from scripts: what each write depends on, and the
//! migration order.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use serde::Serialize;

use crate::relation::{RelationName, listing};
use crate::script::{Finding, Script, Scripted};

/// The writes of scripts in migration order; what their reading and their order find; and
/// the number of statements skipped.
#[derive(Serialize)]
pub struct Graph {
    pub writes: Vec<Scripted>,
    pub findings: Vec<Finding>,
    pub skipped: usize,
}

/// Tarjan's search for the strongly connected components of nodes, such as writes, by
/// `depends`, the nodes that each depends on.
pub struct Search<'a> {
    depends: &'a [Vec<usize>],
    /// Each write's place in the search, none before it is searched, `usize::MAX` once it
    /// is in a component.
    places: Vec<Option<usize>>,
    searched: usize,
    /// The writes searched and in no component yet.
    stack: Vec<usize>,
    /// Each write's component.
    component: Vec<usize>,
    /// The components, each after those that it depends on, its writes in reading order.
    components: Vec<Vec<usize>>,
}

impl Search<'_> {
    /// The strongly connected components of the nodes by `depends`, the nodes that each
    /// depends on: each node's component, and the components, each after those that it
    /// depends on, its nodes in order.
    pub fn components(depends: &[Vec<usize>]) -> (Vec<usize>, Vec<Vec<usize>>) {
        let count = depends.len();
        let (places, component) = (vec![None; count], vec![0; count]);
        let mut search = Search { depends, places, searched: 0, stack: Vec::new(), component, components: Vec::new() };
        for write in 0..count {
            if search.places[write].is_none() {
                search.connect(write);
            }
        }
        (search.component, search.components)
    }

    /// Searches from `write`, closing each component that it finds; the lowest place that
    /// it reaches.
    fn connect(&mut self, write: usize) -> usize {
        let (place, depends) = (self.searched, self.depends);
        self.searched += 1;
        self.places[write] = Some(place);
        self.stack.push(write);
        let mut low = place;
        for &depended in &depends[write] {
            let reached = match self.places[depended] {
                Some(reached) => reached,
                None => self.connect(depended),
            };
            low = low.min(reached);
        }
        if low == place {
            let at = self.stack.iter().rposition(|&other| other == write).expect("a write searched is stacked");
            let mut members = self.stack.split_off(at);
            members.sort_unstable();
            for &member in &members {
                (self.places[member], self.component[member]) = (Some(usize::MAX), self.components.len());
            }
            self.components.push(members);
        }
        low
    }
}

impl Graph {
    /// The graph of `scripts`, their writes in reading order. A write depends on every
    /// write of each relation that it reads, other than its own target, and on the writes
    /// of its own target before it. The strongly connected components of these
    /// dependencies, found in one pass, come in order: among those whose dependencies are
    /// all ordered, the one with the earliest write next, its writes in reading order. A
    /// component of more than one write is a cycle, reported with its writes' targets; so
    /// is a relation with more than one replace write, with those writes.
    pub fn of(scripts: Vec<Script>) -> Graph {
        let skipped = scripts.iter().map(|script| script.skipped).sum();
        let (writes, findings): (Vec<_>, Vec<_>) =
            scripts.into_iter().map(|script| (script.writes, script.findings)).unzip();
        let (writes, mut findings): (Vec<Scripted>, Vec<Finding>) =
            (writes.into_iter().flatten().collect(), findings.into_iter().flatten().collect());
        let mut by_target: HashMap<&RelationName, Vec<usize>> = HashMap::new();
        for (at, write) in writes.iter().enumerate() {
            by_target.entry(&write.target).or_default().push(at);
        }
        let writing = |relation: &RelationName| by_target.get(relation).into_iter().flatten().copied();
        let depends: Vec<Vec<usize>> = (writes.iter().enumerate())
            .map(|(at, write)| {
                let read = write.reads.iter().filter(|relation| **relation != write.target).flat_map(writing);
                read.chain(writing(&write.target).take_while(|&earlier| earlier < at)).collect()
            })
            .collect();
        let (component, components) = Search::components(&depends);
        // The dependencies of each component on the others, which it waits on.
        let mut waiting = vec![0; components.len()];
        let mut waiters = vec![Vec::new(); components.len()];
        let edges = depends.iter().enumerate().flat_map(|(at, needs)| needs.iter().map(move |&need| (need, at)));
        for (need, waiter) in edges.map(|(need, at)| (component[need], component[at])).filter(|(need, at)| need != at) {
            waiting[waiter] += 1;
            waiters[need].push(waiter);
        }
        let mut free: BinaryHeap<Reverse<(usize, usize)>> =
            (0..components.len()).filter(|&at| waiting[at] == 0).map(|at| Reverse((components[at][0], at))).collect();
        let mut sequence = Vec::new();
        while let Some(Reverse((_, at))) = free.pop() {
            sequence.push(at);
            for &waiter in &waiters[at] {
                waiting[waiter] -= 1;
                if waiting[waiter] == 0 {
                    free.push(Reverse((components[waiter][0], waiter)));
                }
            }
        }
        let ids = |members: &[usize]| members.iter().map(|&at| writes[at].id.clone()).collect::<Vec<_>>();
        let cycles = sequence.iter().map(|&at| &components[at]).filter(|members| members.len() > 1);
        findings.extend(cycles.map(|members| {
            let mut seen = HashSet::new();
            let targets = members.iter().map(|&at| &writes[at].target).filter(|target| seen.insert(*target));
            Finding::Cycle { writes: ids(members), relations: targets.cloned().collect() }
        }));
        let replaces = by_target.values().map(|all| all.iter().copied().filter(|&at| writes[at].replaces).collect());
        let mut twice: Vec<Vec<usize>> = replaces.filter(|replaces: &Vec<usize>| replaces.len() > 1).collect();
        twice.sort_unstable();
        findings.extend(twice.iter().map(|replaces| Finding::ReplacedTwice {
            relation: writes[replaces[0]].target.clone(),
            writes: ids(replaces),
        }));
        let mut slots: Vec<Option<Scripted>> = writes.into_iter().map(Some).collect();
        let order = sequence.iter().flat_map(|&at| &components[at]);
        let writes = order.map(|&at| slots[at].take().expect("each write in one component")).collect();
        Graph { writes, findings, skipped }
    }

    /// The graph as JSON, or as text: one line per write in migration order,
    /// `<id> <target> <- <relation>, ...`; one per finding; then the number of statements
    /// skipped.
    pub fn printed(&self, json: bool) -> String {
        if json {
            return serde_json::to_string(self).expect("a graph serialises");
        }
        let writes = (self.writes.iter())
            .map(|write| format!("{} {} <- {}", write.id, write.target, listing(&write.reads)).trim_end().to_string());
        let findings = self.findings.iter().map(ToString::to_string);
        let skipped = format!("skipped: {}", self.skipped);
        writes.chain(findings).chain([skipped]).collect::<Vec<_>>().join("\n")
    }
}
