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
fn a_where_decides_rows() {
    assert_eq!(trace(&[("s", &["a", "b"])], "select a from s where b > 0"), ["s.a -> t.a copy", "s.b -> t.a rows"]);
}

#[test]
fn a_join_condition_decides_rows() {
    let found = trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], "select s.a, u.c from s join u on s.k = u.k");
    let expected = [
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.k -> t.a rows",
        "s.k -> t.c rows",
        "u.c -> t.c copy",
        "u.k -> t.c rows",
    ];
    assert_eq!(found, expected);
}

#[test]
fn group_by_decides_rows_of_the_other_columns() {
    let found = trace(&[("s", &["k", "a"])], "select k, sum(a) as total from s group by k");
    assert_eq!(found, ["s.k -> t.k copy", "s.a -> t.total derive", "s.k -> t.total rows"]);
}

#[test]
fn having_decides_rows() {
    let found = trace(&[("s", &["k", "a"])], "select k from s group by k having sum(a) > 0");
    assert_eq!(found, ["s.a -> t.k rows", "s.k -> t.k copy"]);
}

#[test]
fn qualify_decides_rows() {
    let found =
        trace(&[("s", &["a", "b", "c"])], "select a from s qualify row_number() over (partition by b order by c) = 1");
    assert_eq!(found, ["s.a -> t.a copy", "s.b -> t.a rows", "s.c -> t.a rows"]);
}

#[test]
fn order_by_under_a_limit_decides_rows() {
    for limit in ["limit 1", "limit 1 offset 2", "fetch first 1 rows only"] {
        let found = trace(&[("s", &["a", "b"])], &format!("select a from s order by b {limit}"));
        assert_eq!(found, ["s.a -> t.a copy", "s.b -> t.a rows"], "{limit}");
    }
}

#[test]
fn a_lateral_table_functions_input_decides_rows() {
    let found =
        trace(&[("s", &["k", "items"])], "select s.k, f.value as v from s, lateral flatten(input => s.items) f");
    assert_eq!(found, ["s.items -> t.k rows", "s.k -> t.k copy", "s.items -> t.v derive"]);
}

#[test]
fn copy_outranks_rows() {
    assert_eq!(trace(&[("s", &["a"])], "select a from s where a > 0"), ["s.a -> t.a copy"]);
}

#[test]
fn derive_outranks_rows() {
    assert_eq!(trace(&[("s", &["a"])], "select a + 1 as x from s where a > 0"), ["s.a -> t.x derive"]);
}

#[test]
fn a_literal_or_count_has_no_parent() {
    assert!(trace(&[("s", &["a"])], "select 1 as one, count(*) as n, current_date as d from s").is_empty());
}

#[test]
fn a_clause_in_a_cte_reaches_every_column() {
    let sql = "with c as (select a from s where b > 0), d as (select a from c) select a from d";
    assert_eq!(trace(&[("s", &["a", "b"])], sql), ["s.a -> t.a copy", "s.b -> t.a rows"]);
}

#[test]
fn a_clause_in_an_unread_cte_is_a_gap() {
    let sql = "with c as (select a from s), d as (select a from s where b > 0), e as (select a from d) select a from c";
    assert_eq!(trace(&[("s", &["a", "b"])], sql), ["s.a -> t.a copy", "gap unattributed_use - -"]);
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
