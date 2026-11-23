// SPDX-License-Identifier: EUPL-1.2
//! The kinds of edges and their reach, as the Edge entry of `docs/concepts.md` and the
//! proof of concept's catalogue of clauses give them.

use serde_json::json;

use super::{outline, request, respond, trace};

#[test]
fn a_derived_table_is_a_step() {
    assert_eq!(trace(&[("s", &["a"])], "select x from (select a as x from s) d"), ["s.a -> t.x copy"]);
}

#[test]
fn each_branch_of_a_union_copies() {
    let found = trace(&[("s", &["a"]), ("u", &["a"])], "select a from s union all select a from u");
    assert_eq!(found, ["s.a -> t.a copy", "u.a -> t.a copy"]);
}

#[test]
fn an_operator_derives() {
    assert_eq!(trace(&[("s", &["a", "b"])], "select a + b as x from s"), ["s.a -> t.x derive", "s.b -> t.x derive"]);
}

#[test]
fn a_cast_derives() {
    assert_eq!(trace(&[("s", &["a"])], "select a::int as x from s"), ["s.a -> t.x derive"]);
}

#[test]
fn case_derives() {
    let found = trace(&[("s", &["a", "b"])], "select case when a > 0 then b else 0 end as x from s");
    assert_eq!(found, ["s.a -> t.x derive", "s.b -> t.x derive"]);
}

#[test]
fn an_aggregate_derives() {
    assert_eq!(trace(&[("s", &["a"])], "select sum(a) as x from s"), ["s.a -> t.x derive"]);
}

#[test]
fn a_window_derives_from_its_partition_order_and_frame() {
    let sql = "select sum(a) over (partition by b order by c rows between 1 preceding and current row) as x from s";
    assert_eq!(trace(&[("s", &["a", "b", "c"])], sql), ["s.a -> t.x derive", "s.b -> t.x derive", "s.c -> t.x derive"]);
}

#[test]
fn a_path_into_a_semi_structured_value_derives() {
    assert_eq!(trace(&[("s", &["payload"])], "select payload:a.b::string as x from s"), ["s.payload -> t.x derive"]);
}

#[test]
fn a_literal_or_count_has_no_parent() {
    assert!(trace(&[("s", &["a"])], "select 1 as one, count(*) as n, current_date as d from s").is_empty());
}

#[test]
fn edges_spell_names_as_given() {
    let answers = respond(&request(&[("Raw.Orders", &["Id"])], &[("Stg.T", "select ID as \"Key\" from raw.orders")]));
    assert_eq!(outline(&answers[0]), ["Raw.Orders.Id -> Stg.T.\"Key\" copy"]);
}

#[test]
fn traces_in_each_built_dialect() {
    for dialect in ["snowflake", "tsql", "duckdb"] {
        let mut request = request(&[("s", &["a"])], &[("t", "select a as b from s")]);
        request["dialect"] = json!(dialect);
        assert_eq!(outline(&respond(&request)[0]), ["s.a -> t.b copy"], "{dialect}");
    }
}
