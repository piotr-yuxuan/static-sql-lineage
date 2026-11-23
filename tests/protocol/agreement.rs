// SPDX-License-Identifier: EUPL-1.2
//! dbt's own lineage of the proof of concept's synthetic projects, the fixture and the
//! probes, against the answers: every difference is explained by a line of the project's
//! `explain.tsv`, written from `definition.md` and dbt's lineage, and every line explains
//! one. Each project of `tests/agreement/` holds its protocol 1 `request.json`, dbt's
//! edges lifted to relations in `dbt.tsv`, and `explain.tsv`; jaffle-shop's, which dbt
//! Labs publishes, stay out of the repository.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use super::{read, respond};

/// The edges by their ends, `parent relation, column, child relation, column`, each with
/// its kind.
type Edges = BTreeMap<[String; 4], String>;

/// The differences between the answers to `request` and dbt's edges `dbt`, each as the
/// first six fields of a line of an `explain.tsv`, that no line of `explain` explains;
/// and the lines of `explain` that explain none. A kind of the answers is `gap` where an
/// answer has a gap that covers the child column, and `-` where it has no edge.
fn unexplained(request: &Value, dbt: &str, explain: &str) -> (Vec<String>, Vec<String>) {
    let (mut ours, mut gaps) = (Edges::new(), BTreeSet::new());
    let writes = request["writes"].as_array().expect("writes");
    for (write, answer) in writes.iter().zip(respond(request)) {
        let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
        for edge in answer["edges"].as_array().expect("edges") {
            let (parent, child) = (&edge["parent"], &edge["child"]);
            let ends =
                [text(&parent["relation"]), text(&parent["column"]), text(&child["relation"]), text(&child["column"])];
            ours.insert(ends, text(&edge["kind"]));
        }
        gaps.extend(
            answer["gaps"].as_array().expect("gaps").iter().map(|gap| (text(&write["target"]), text(&gap["column"]))),
        );
    }
    let theirs: Edges = (dbt.lines())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            ([0, 1, 2, 3].map(|at| fields[at].to_string()), fields[4].to_string())
        })
        .collect();
    let explanations: Vec<&str> = explain.lines().filter(|line| !line.is_empty() && !line.starts_with('#')).collect();
    let mut used = vec![false; explanations.len()];
    let mut differences = Vec::new();
    for ends in ours.keys().chain(theirs.keys()).collect::<BTreeSet<_>>() {
        let covered =
            gaps.contains(&(ends[2].clone(), String::new())) || gaps.contains(&(ends[2].clone(), ends[3].clone()));
        let kind = ours.get(ends).map_or(if covered { "gap" } else { "-" }, String::as_str);
        let dbt = theirs.get(ends).map_or("-", String::as_str);
        if kind == dbt {
            continue;
        }
        let difference = format!("{}\t{dbt}\t{kind}", ends.join("\t"));
        match explanations.iter().position(|line| line.split('\t').take(6).eq(difference.split('\t'))) {
            Some(at) => used[at] = true,
            None => differences.push(difference),
        }
    }
    let unused = (explanations.iter().zip(used)).filter(|(_, used)| !used).map(|(line, _)| line.to_string()).collect();
    (differences, unused)
}

/// The three files of the project at `directory`.
fn project(directory: &Path) -> (Value, String, String) {
    let file = |name: &str| {
        let path = directory.join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    };
    (serde_json::from_str(&file("request.json")).expect("the request is JSON"), file("dbt.tsv"), file("explain.tsv"))
}

/// Whether every difference with dbt on the project at `directory` is explained, and every
/// explanation used.
fn assert_agrees(directory: &Path) {
    let (request, dbt, explain) = project(directory);
    let (differences, unused) = unexplained(&request, &dbt, &explain);
    assert!(differences.is_empty(), "unexplained:\n{}", differences.join("\n"));
    assert!(unused.is_empty(), "explains nothing:\n{}", unused.join("\n"));
}

#[test]
fn agrees_with_dbt_on_the_fixture() {
    assert_agrees(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agreement/fixture"));
}

#[test]
fn agrees_with_dbt_on_the_probes() {
    assert_agrees(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agreement/probes"));
}

#[test]
fn an_unexplained_difference_fails() {
    let request: Value = serde_json::from_str(&read("tests/agreement/fixture/request.json")).expect("JSON");
    let made_up = "\"shop\".\"main\".\"raw_orders\"\tstatus\t\"shop\".\"main\".\"stg_orders\"\torder_id\trows";
    let dbt = format!("{}{made_up}\n", read("tests/agreement/fixture/dbt.tsv"));
    let (differences, unused) = unexplained(&request, &dbt, &read("tests/agreement/fixture/explain.tsv"));
    assert_eq!(differences, [format!("{made_up}\t-")]);
    assert!(unused.is_empty(), "{unused:?}");
}

#[test]
fn an_explanation_that_explains_nothing_fails() {
    let request: Value = serde_json::from_str(&read("tests/agreement/fixture/request.json")).expect("JSON");
    let made_up =
        "\"shop\".\"main\".\"raw_orders\"\tstatus\t\"shop\".\"main\".\"stg_orders\"\torder_id\trows\t-\tmeaning\tnone";
    let (differences, unused) = unexplained(&request, &read("tests/agreement/fixture/dbt.tsv"), made_up);
    assert!(differences.is_empty(), "{differences:?}");
    assert_eq!(unused, [made_up]);
}

#[test]
#[ignore = "jaffle-shop is dbt Labs' project, kept out of the repository: run with SQL_LINEAGE_JAFFLE_SHOP"]
fn agrees_with_dbt_on_jaffle_shop() {
    let directory =
        std::env::var("SQL_LINEAGE_JAFFLE_SHOP").expect("SQL_LINEAGE_JAFFLE_SHOP names jaffle-shop's three files");
    assert_agrees(Path::new(&directory));
}
