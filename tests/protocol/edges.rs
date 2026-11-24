// SPDX-License-Identifier: EUPL-1.2
//! The kinds of edges and their reach, as the Edge entry of `docs/concepts.md` and the
//! proof of concept's catalogue of clauses give them.

use super::{outline, request, respond, trace, trace_in, traces};

cases! {
    a_derived_table_is_a_step: traces(&[("s", &["a"])], "select x from (select a as x from s) d", &["s.a -> t.x copy"]);
    each_branch_of_a_union_copies: traces(&[("s", &["a"]), ("u", &["a"])], "select a from s union all select a from u",
        &["s.a -> t.a copy", "u.a -> t.a copy"]);
    an_operator_derives:
        traces(&[("s", &["a", "b"])], "select a + b as x from s", &["s.a -> t.x derive", "s.b -> t.x derive"]);
    a_cast_derives: traces(&[("s", &["a"])], "select a::int as x from s", &["s.a -> t.x derive"]);
    case_derives: traces(&[("s", &["a", "b"])], "select case when a > 0 then b else 0 end as x from s",
        &["s.a -> t.x derive", "s.b -> t.x derive"]);
    an_aggregate_derives: traces(&[("s", &["a"])], "select sum(a) as x from s", &["s.a -> t.x derive"]);
    a_window_derives_from_its_partition_order_and_frame: traces(&[("s", &["a", "b", "c"])],
        "select sum(a) over (partition by b order by c rows between 1 preceding and current row) as x from s",
        &["s.a -> t.x derive", "s.b -> t.x derive", "s.c -> t.x derive"]);
    a_path_into_a_semi_structured_value_derives:
        traces(&[("s", &["payload"])], "select payload:a.b::string as x from s", &["s.payload -> t.x derive"]);
    a_where_decides_rows:
        traces(&[("s", &["a", "b"])], "select a from s where b > 0", &["s.a -> t.a copy", "s.b -> t.a rows"]);
    a_join_condition_decides_rows: traces(&[("s", &["k", "a"]), ("u", &["k", "c"])],
        "select s.a, u.c from s join u on s.k = u.k", &[
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.k -> t.a rows",
        "s.k -> t.c rows",
        "u.c -> t.c copy",
        "u.k -> t.c rows",
    ]);
    group_by_decides_rows_of_the_other_columns: traces(&[("s", &["k", "a"])],
        "select k, sum(a) as total from s group by k",
        &["s.k -> t.k copy", "s.a -> t.total derive", "s.k -> t.total rows"]);
    having_decides_rows: traces(&[("s", &["k", "a"])], "select k from s group by k having sum(a) > 0",
        &["s.a -> t.k rows", "s.k -> t.k copy"]);
    qualify_decides_rows: traces(&[("s", &["a", "b", "c"])],
        "select a from s qualify row_number() over (partition by b order by c) = 1",
        &["s.a -> t.a copy", "s.b -> t.a rows", "s.c -> t.a rows"]);
    order_by_under_a_limit_decides_rows: for limit in ["limit 1", "limit 1 offset 2", "fetch first 1 rows only"] {
        let sql = format!("select a from s order by b {limit}");
        traces(&[("s", &["a", "b"])], &sql, &["s.a -> t.a copy", "s.b -> t.a rows"]);
    };
    a_lateral_table_functions_input_decides_rows: traces(&[("s", &["k", "items"])],
        "select s.k, f.value as v from s, lateral flatten(input => s.items) f",
        &["s.items -> t.k rows", "s.k -> t.k copy", "s.items -> t.v derive"]);
    copy_outranks_rows: traces(&[("s", &["a"])], "select a from s where a > 0", &["s.a -> t.a copy"]);
    derive_outranks_rows: traces(&[("s", &["a"])], "select a + 1 as x from s where a > 0", &["s.a -> t.x derive"]);
    a_literal_or_count_has_no_parent:
        assert!(trace(&[("s", &["a"])], "select 1 as one, count(*) as n, current_date as d from s").is_empty());
    a_clause_in_a_cte_reaches_every_column: traces(&[("s", &["a", "b"])],
        "with c as (select a from s where b > 0), d as (select a from c) select a from d",
        &["s.a -> t.a copy", "s.b -> t.a rows"]);
    a_clause_in_an_unread_cte_is_a_gap: traces(&[("s", &["a", "b"])],
        "with c as (select a from s), d as (select a from s where b > 0), e as (select a from d) select a from c",
        &["s.a -> t.a copy", "gap unattributed_use - -"]);
}

#[test]
fn edges_spell_names_as_given() {
    let answers = respond(&request(&[("Raw.Orders", &["Id"])], &[("Stg.T", "select ID as \"Key\" from raw.orders")]));
    assert_eq!(outline(&answers[0]), ["Raw.Orders.Id -> Stg.T.\"Key\" copy"]);
}

#[test]
fn traces_in_each_built_dialect() {
    for dialect in ["snowflake", "tsql", "duckdb"] {
        assert_eq!(trace_in(dialect, &[("s", &["a"])], "select a as b from s"), ["s.a -> t.b copy"], "{dialect}");
    }
}
