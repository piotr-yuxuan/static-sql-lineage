// SPDX-License-Identifier: EUPL-1.2
//! Writes given as statements, as D1 to D7 of plan 006 and the Write entry of
//! `docs/concepts.md` read them: replace statements, contribute statements, and the
//! statements that are not writes. Expectations from those, the owner's rulings on Q3 and
//! Q4, and the probe's checks on Snowflake.

use serde_json::{Value, json};

use super::{outline, request, respond};

/// The probe's relations: `s` and `u` read, `t` the target of most writes.
const CATALOG: &[(&str, &[&str])] = &[("s", &["k", "x", "y", "flag"]), ("u", &["k", "z"]), ("t", &["k", "a", "b"])];

/// The request of `writes`, each a target, also its id, and its SQL, in `dialect`
/// against `CATALOG`.
fn writes(dialect: &str, writes: &[(&str, &str)]) -> Value {
    let mut request = request(CATALOG, writes);
    request["dialect"] = json!(dialect);
    request
}

/// The outline of each answer to `writes` in `dialect`.
fn traced(dialect: &str, writes: &[(&str, &str)]) -> Vec<Vec<String>> {
    respond(&self::writes(dialect, writes)).iter().map(outline).collect()
}

/// The outline of the answer to `sql`, its target `t`, in `dialect`.
fn trace(dialect: &str, sql: &str) -> Vec<String> {
    traced(dialect, &[("t", sql)]).remove(0)
}

#[test]
fn replace_statements_are_replace_writes() {
    let query = "select k, x from s";
    let mut forms: Vec<(&str, String)> = (["view", "or replace view", "secure view", "materialized view"].iter())
        .chain(&["view if not exists", "table", "or replace table", "temporary table", "transient table"])
        .map(|kind| ("snowflake", format!("create {kind} t as {query}")))
        .collect();
    forms.push(("snowflake", format!("create dynamic table t target_lag = '1 minute' warehouse = w as {query}")));
    for kind in ["create view", "create or alter view", "alter view"] {
        forms.push(("tsql", format!("{kind} t as {query}")));
    }
    forms.push(("tsql", "select k, x into t from s".to_string()));
    for kind in ["view", "or replace view", "table", "or replace table"] {
        forms.push(("duckdb", format!("create {kind} t as {query}")));
    }
    let copied = ["s.k -> t.k copy", "s.x -> t.x copy"];
    for (dialect, sql) in &forms {
        // The target's columns become the query's: `x`, not the catalog's `a` and `b`.
        let found = traced(dialect, &[("t", sql), ("v", "select x from t")]);
        assert_eq!(found, [copied.to_vec(), vec!["t.x -> v.x copy"]], "{dialect}: {sql}");
    }
    // polyglot keeps the `INTO` of a set operation on its left branch.
    let union = "select k, x into t from s union all select k, z from u";
    assert_eq!(trace("tsql", union), ["s.k -> t.k copy", "u.k -> t.k copy", "s.x -> t.x copy", "u.z -> t.x copy"]);
    let mut temporary = writes("tsql", &[("#t", "select k, x into #t from s")]);
    temporary["catalog"] = json!([{"relation": "s", "columns": ["k", "x"]}]);
    assert_eq!(outline(&respond(&temporary)[0]), ["s.k -> #t.k copy", "s.x -> #t.x copy"]);
}

#[test]
fn a_view_answers_as_its_bare_query() {
    let query = "with c as (select k, x from s where flag = 1) \
        select c.k, c.x + u.z as total, (select max(y) from s) as top from c join u on c.k = u.k";
    let bare = &respond(&writes("snowflake", &[("t", query)]))[0];
    let view = &respond(&writes("snowflake", &[("t", &format!("create view t as {query}"))]))[0];
    assert!(!outline(bare).is_empty());
    assert_eq!(view, bare);
}

#[test]
fn a_view_column_list_renames_its_outputs() {
    let answer = &respond(&writes("snowflake", &[("t", "create view t (kk, \"Xx\") as select k, x from s")]))[0];
    assert_eq!(answer["columns"], json!(["kk", "\"Xx\""]));
    assert_eq!(outline(answer), ["s.k -> t.kk copy", "s.x -> t.\"Xx\" copy"]);
    let found = trace("tsql", "create view t (kk, [Xx]) as select k, x from s");
    assert_eq!(found, ["s.k -> t.kk copy", "s.x -> t.[Xx] copy"]);
}

#[test]
fn a_table_column_list_renames_its_outputs() {
    let found = traced(
        "snowflake",
        &[("t", "create table t (kk int, \"Aa\" int) as select k, x + 1 from s"), ("v", "select \"Aa\" from t")],
    );
    assert_eq!(found, [vec!["s.k -> t.kk copy", "s.x -> t.\"Aa\" derive"], vec!["t.\"Aa\" -> v.\"Aa\" copy"]]);
}

#[test]
fn a_column_list_of_another_length_is_refused() {
    for sql in
        ["create view t (kk) as select k, x from s", "create table t (kk int, aa int, bb int) as select k, x from s"]
    {
        assert_eq!(trace("snowflake", sql), ["refused invalid"], "{sql}");
    }
}

#[test]
fn a_statement_for_another_target_is_refused() {
    assert_eq!(trace("snowflake", "create view v as select k, x from s"), ["refused other_target"]);
    // Traced under its request's target before plan 006, a name that its answer got wrong.
    assert_eq!(trace("tsql", "select k, x into v from s"), ["refused other_target"]);
    // A statement's target is completed by the defaults, as the request's is.
    let mut request = writes("snowflake", &[("t", "create view db.sch.t as select k from s")]);
    request["defaults"] = json!({"database": "db", "schema": "sch"});
    request["catalog"] = json!([{"relation": "s", "columns": ["k"]}]);
    assert_eq!(outline(&respond(&request)[0]), ["s.k -> t.k copy"]);
}

#[test]
fn statements_that_are_not_writes_are_refused() {
    let statements = [
        ("snowflake", "insert into t (k, a) values (1, 2)"),
        ("snowflake", "create table t (k int, a int)"),
        ("duckdb", "insert into t by name select x as a, k from s"),
        ("snowflake", "insert all into t (k, a) values (k, x) select k, x from s"),
        ("snowflake", "truncate table t"),
    ];
    for (dialect, sql) in statements {
        assert_eq!(trace(dialect, sql), ["refused not_a_query"], "{dialect}: {sql}");
    }
}

#[test]
fn a_statement_that_is_not_a_write_keeps_its_target() {
    for sql in ["insert into t (k, a) values (1, 2)", "create view v as select k, x from s"] {
        let found = traced("snowflake", &[("t", sql), ("w", "select b from t")]);
        assert_eq!(found[1], ["t.b -> w.b copy"], "{sql}");
    }
}
