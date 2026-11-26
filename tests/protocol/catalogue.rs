// SPDX-License-Identifier: EUPL-1.2
//! The clauses of the proof of concept's catalogue that its Snowflake cases leave out,
//! as the Edge entry of `docs/concepts.md` and its `definition.md` give them: uses in
//! subqueries, filtering branches, keys that a join merges, an aggregate's order; and
//! the gaps or refusals of the clauses that polyglot does not trace.

use serde_json::json;

use super::{answered, trace, traces, traces_in};

/// The answer to `select k, a, c from s <join> join u using (k)`.
fn using(join: &str) -> Vec<String> {
    trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], &format!("select k, a, c from s {join} join u using (k)"))
}

/// The edges of `using`'s `a` and `c`, which the join's keys decide the rows of.
const USING_ROWS: [&str; 6] =
    ["s.a -> t.a copy", "s.k -> t.a rows", "u.k -> t.a rows", "s.k -> t.c rows", "u.c -> t.c copy", "u.k -> t.c rows"];

cases! {
    a_where_subquery_decides_rows_of_every_column: traces(&[("s", &["a", "b"]), ("u", &["x", "c"])],
        "select a, b from s where a > (select max(x) from u where c > 0)", &[
        "s.a -> t.a copy",
        "u.c -> t.a rows",
        "u.x -> t.a rows",
        "s.a -> t.b rows",
        "s.b -> t.b copy",
        "u.c -> t.b rows",
        "u.x -> t.b rows",
    ]);
    a_scalar_subquery_reaches_only_its_column: traces(&[("s", &["a"]), ("u", &["x", "c"])],
        "select a, (select max(x) from u where c > 0) as m from s",
        &["s.a -> t.a copy", "u.c -> t.m rows", "u.x -> t.m derive"]);
    a_scalar_subquery_after_a_star_is_a_gap: traces(&[("s", &["a"]), ("u", &["x", "c"])],
        "select *, (select max(x) from u where c > 0) as m from s",
        &["s.a -> t.a copy", "u.x -> t.m derive", "gap unattributed_use - -"]);
    a_scalar_subquery_in_a_cte_reaches_the_columns_that_read_it: traces(&[("s", &["k", "a"]), ("u", &["k", "x"])],
        "with c as (select a, (select max(x) from u where u.k = s.k) as m from s) select m as top, a from c",
        &["s.k -> t.top rows", "u.k -> t.top rows", "u.x -> t.top derive", "s.a -> t.a copy"]);
    a_nested_subquery_reaches_what_its_holder_reaches: traces(
        &[("s", &["a", "b"]), ("u", &["k", "x"]), ("v", &["k", "z"])],
        "select (select max(x) from u where u.k in (select k from v where v.z = s.a)) as m, b from s", &[
        "s.a -> t.m rows",
        "u.k -> t.m rows",
        "u.x -> t.m derive",
        "v.k -> t.m rows",
        "v.z -> t.m rows",
        "s.b -> t.b copy",
    ]);
    in_decides_rows: for in_ in ["in", "not in"] {
        let sql = format!("select a from s where k {in_} (select k from u where c > 0)");
        traces(&[("s", &["a", "k"]), ("u", &["k", "c"])], &sql,
            &["s.a -> t.a copy", "s.k -> t.a rows", "u.c -> t.a rows", "u.k -> t.a rows"]);
    };
    exists_decides_rows: for exists in ["exists", "not exists"] {
        let sql = format!("select a from s where {exists} (select 1 from u where u.k = s.k)");
        traces(&[("s", &["a", "k"]), ("u", &["k"])], &sql, &["s.a -> t.a copy", "s.k -> t.a rows", "u.k -> t.a rows"]);
    };
    any_and_all_decide_rows: for compared in ["b = any", "b > all"] {
        let sql = format!("select a from s where {compared} (select x from u where c > 0)");
        traces(&[("s", &["a", "b"]), ("u", &["x", "c"])], &sql,
            &["s.a -> t.a copy", "s.b -> t.a rows", "u.c -> t.a rows", "u.x -> t.a rows"]);
    };
    a_correlated_column_decides_rows: traces(&[("s", &["k"]), ("u", &["k", "x"])],
        "select k, (select max(x) from u where u.k = s.k) as m from s",
        &["s.k -> t.k copy", "s.k -> t.m rows", "u.k -> t.m rows", "u.x -> t.m derive"]);
    an_outer_column_in_a_scalar_select_list_is_a_gap: traces(&[("s", &["a", "b", "k"]), ("u", &["k", "x"])],
        "select a, (select s.b + max(x) from u where u.k = s.k) as m from s",
        &["s.a -> t.a copy", "s.k -> t.m rows", "u.k -> t.m rows", "u.x -> t.m derive", "gap unresolved_reference - m"],
    );
    a_tuple_in_is_a_gap: traces(&[("s", &["k", "a", "b"]), ("u", &["x", "c"])],
        "select k from s where (a, b) in (select x, c from u)", &["s.k -> t.k copy", "gap unattributed_use - -"]);
    the_filter_branch_of_intersect_decides_rows: traces(&[("s", &["a"]), ("u", &["x"])],
        "select a from s intersect select x from u", &["s.a -> t.a copy", "u.x -> t.a rows"]);
    the_filter_branch_of_except_decides_rows: traces(&[("s", &["a", "b"]), ("u", &["x", "c"])],
        "select a, b from s except select x, c from u", &[
        "s.a -> t.a copy",
        "u.c -> t.a rows",
        "u.x -> t.a rows",
        "s.b -> t.b copy",
        "u.c -> t.b rows",
        "u.x -> t.b rows",
    ]);
    minus_is_except: traces(&[("s", &["a"]), ("u", &["x"])], "select a from s minus select x from u",
        &["s.a -> t.a copy", "u.x -> t.a rows"]);
    a_filter_branch_in_a_cte_decides_rows: traces(&[("s", &["k"]), ("u", &["k"])],
        "with c as (select k from s intersect select k from u) select k from c",
        &["s.k -> t.k copy", "u.k -> t.k rows"]);
    a_filter_branch_in_a_chain_decides_rows: {
        let catalog: &[(&str, &[&str])] = &[("s", &["a"]), ("u", &["x"]), ("v", &["z"])];
        traces(catalog, "select a from s union select x from u except select z from v",
            &["s.a -> t.a copy", "u.x -> t.a copy", "v.z -> t.a rows"]);
        traces(catalog, "select a from s except select x from u union all select z from v",
            &["s.a -> t.a copy", "u.x -> t.a rows", "v.z -> t.a copy"]);
    };
    a_subquery_in_a_filter_branch_decides_rows_of_every_column: traces(
        &[("s", &["a", "b"]), ("u", &["x"]), ("v", &["z", "c"])],
        "select a, b from s except select x, (select max(z) from v where c > 0) from u", &[
        "s.a -> t.a copy",
        "u.x -> t.a rows",
        "v.c -> t.a rows",
        "v.z -> t.a rows",
        "s.b -> t.b copy",
        "u.x -> t.b rows",
        "v.c -> t.b rows",
        "v.z -> t.b rows",
    ]);
    an_inner_using_key_copies_both:
        assert_eq!(using("inner"), [&["s.k -> t.k copy", "u.k -> t.k copy"][..], &USING_ROWS].concat());
    a_left_using_key_copies_the_left_and_the_right_decides_rows:
        assert_eq!(using("left"), [&["s.k -> t.k copy", "u.k -> t.k rows"][..], &USING_ROWS].concat());
    a_right_using_key_mirrors_the_left:
        assert_eq!(using("right"), [&["s.k -> t.k rows", "u.k -> t.k copy"][..], &USING_ROWS].concat());
    a_full_using_key_derives_from_both:
        assert_eq!(using("full"), [&["s.k -> t.k derive", "u.k -> t.k derive"][..], &USING_ROWS].concat());
    a_using_key_over_ctes_finds_its_sides: traces(&[("s", &["k", "a"]), ("u", &["k", "x"])],
        "with o as (select k as key, a from s), c as (select k as key, x from u) \
         select key, a, x from o left join c using (key)", &[
        "s.k -> t.key copy",
        "u.k -> t.key rows",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.k -> t.a rows",
        "s.k -> t.x rows",
        "u.k -> t.x rows",
        "u.x -> t.x copy",
    ]);
    a_natural_left_key_copies_the_left: traces(&[("s", &["k", "a", "b"]), ("w", &["k", "a", "y"])],
        "select k, a from s natural left join w", &[
        "s.a -> t.k rows",
        "s.k -> t.k copy",
        "w.a -> t.k rows",
        "w.k -> t.k rows",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "w.a -> t.a rows",
        "w.k -> t.a rows",
    ]);
    a_key_merged_in_a_cte_follows_its_join: for sql in [
        "with o as (select k, a from s), c as (select k, x from u), \
         f as (select * from o left join c using (k)) select * from f",
        "select * from (select * from s left join u using (k)) d",
    ] {
        traces(&[("s", &["k", "a"]), ("u", &["k", "x"])], sql, &[
            "s.k -> t.k copy",
            "u.k -> t.k rows",
            "s.a -> t.a copy",
            "s.k -> t.a rows",
            "u.k -> t.a rows",
            "s.k -> t.x rows",
            "u.k -> t.x rows",
            "u.x -> t.x copy",
        ]);
    };
    a_key_merged_in_a_branch_is_a_gap_for_its_column_alone: traces(&[("s", &["k", "a"]), ("u", &["k", "c"])],
        "select k, a from s join u using (k) union all select k, c from u", &[
        "s.k -> t.k rows",
        "u.k -> t.k rows",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.c -> t.a copy",
        "u.k -> t.a rows",
        "gap unattributed_use - k",
    ]);
    a_key_merged_across_three_sources_is_a_gap: traces(&[("s", &["k"]), ("u", &["k"]), ("v", &["k"])],
        "select k from s join u using (k) join v using (k)",
        &["s.k -> t.k rows", "u.k -> t.k rows", "v.k -> t.k rows", "gap unattributed_use - k"]);
    an_aggregate_order_derives: traces_in("duckdb", &[("s", &["k", "a", "b"])],
        "select k, string_agg(a, ',' order by b) as l from s group by k",
        &["s.k -> t.k copy", "s.a -> t.l derive", "s.b -> t.l derive", "s.k -> t.l rows"]);
    within_group_derives: traces(&[("s", &["k", "a", "b"])],
        "select k, listagg(a, ',') within group (order by b) as l from s group by k",
        &["s.k -> t.k copy", "s.a -> t.l derive", "s.b -> t.l derive", "s.k -> t.l rows"]);
    pivot_is_a_gap: answered(&[("sales", &["region", "quarter", "amount"])],
        "select p.region as region from sales pivot (sum(amount) for quarter in ('Q1', 'Q2')) as p",
        json!(["region"]), &["gap analysis_failed - -"]);
    unpivot_is_a_gap: traces(&[("wide", &["id", "a", "b"])],
        "select id, metric, val from wide unpivot (val for metric in (a, b))", &["gap analysis_failed - -"]);
    a_star_as_an_argument_derives_from_every_column: traces(USERS, "select id, hash(*) as obj from users",
        &["users.id -> t.id copy", "users.id -> t.obj derive", "users.name -> t.obj derive"]);
    a_star_as_an_argument_in_a_cte_is_a_gap: traces_a_gap_among("snowflake", USERS,
        "with c as (select id, hash(*) as h from users) select id, h from c",
        &["users.id -> t.id copy"], &["users.id -> t.h derive", "users.name -> t.h derive"]);
    a_star_as_an_argument_elsewhere_is_a_gap: {
        let all = ["users.id -> t.id copy", "users.name -> t.name copy"];
        let fed = ["users.id -> t.f derive", "users.name -> t.f derive"];
        for (dialect, function) in [
            ("snowflake", "array_construct(*)"), ("snowflake", "array_construct(*, name)"),
            ("snowflake", "hash(*) + id"), ("databricks", "struct(*)"), ("databricks", "hash(*)"),
            ("databricks", "concat(name, to_json(struct(*)))"),
            ("databricks", "to_json(named_struct('i', id, 'all', struct(*)))"),
        ] {
            traces_a_gap_among(dialect, USERS, &format!("select id, {function} as f from users"), &all[..1], &fed);
        }
        traces_a_gap_among("snowflake", USERS, "select *, array_construct(*) as f from users", &all, &fed);
        traces_a_gap_among("databricks", USERS, "select users.*, struct(*) as f from users", &all, &fed);
        traces_a_gap_among("duckdb", USERS, "select id from users where hash(*) > 0",
            &["users.id -> t.id copy"], &["users.name -> t.id rows"]);
    };
    a_star_as_an_argument_derives_beside_a_column_with_no_parent: for sql in [
        "select current_timestamp() as loaded_at, object_construct(*) as o from users",
        "select object_construct(*) as o, 1 as one from users",
    ] {
        traces(USERS, sql, &["users.id -> t.o derive", "users.name -> t.o derive"]);
    };
    star_ilike_matches_its_pattern_ignoring_case: {
        traces(USERS, "select * ilike 'N_ME' from users", &["users.name -> t.name copy"]);
        traces(USERS, "select * ilike 'I%' from users", &["users.id -> t.id copy"]);
    };
    an_outer_key_of_a_relation_joined_to_itself_copies: for (join, expected) in [
        ("left", &["s.k -> t.k copy"][..]),
        ("right", &["s.k -> t.k copy"]),
        ("inner", &["s.k -> t.k rows", "gap unattributed_use - k"]),
        ("full", &["s.k -> t.k rows", "gap unattributed_use - k"]),
    ] {
        traces(&[("s", &["k", "a"])], &format!("select k from s x {join} join s y using (k)"), expected);
    };
    a_left_chain_of_three_copies_the_first_key: traces(&[("s", &["k"]), ("u", &["k"]), ("w", &["k"])],
        "select k from s left join u using (k) left join w using (k)",
        &["s.k -> t.k copy", "u.k -> t.k rows", "w.k -> t.k rows"]);
    a_left_join_of_derived_tables_copies_the_kept_key: traces(&[("s", &["k"]), ("u", &["k"])],
        "select k from (select k from s) x left join (select k from u) y using (k)",
        &["s.k -> t.k copy", "u.k -> t.k rows"]);
}

/// The relation of the rows of a `*` as a function's argument.
const USERS: &[(&str, &[&str])] = &[("users", &["id", "name"])];

/// Asserts that the answer to `sql` against `catalog` in `dialect`, its target `t`, gives
/// the edges `required`, in their order, and a `wildcard` gap, and no other line but edges
/// among `fed`, those that the Edge entry gives from the `*`.
#[track_caller]
fn traces_a_gap_among(dialect: &str, catalog: &[(&str, &[&str])], sql: &str, required: &[&str], fed: &[&str]) {
    let found = super::trace_in(dialect, catalog, sql);
    let rest: Vec<&str> = found.iter().map(String::as_str).filter(|line| !fed.contains(line)).collect();
    assert_eq!(rest, [required, &["gap wildcard - -"]].concat(), "{sql}: {found:?}");
}

/// For each join that merges `s` and `u` on `k`, the edges of `s.k` and `u.k` to the column
/// `column` of `t`, a transformed key.
fn joins(column: &str) -> [(&'static str, [String; 2]); 4] {
    let edges = |s: &str, u: &str| [format!("s.k -> t.{column} {s}"), format!("u.k -> t.{column} {u}")];
    [
        ("inner", edges("derive", "derive")),
        ("left", edges("derive", "rows")),
        ("right", edges("rows", "derive")),
        ("full", edges("derive", "derive")),
    ]
}

#[test]
fn a_transformed_key_through_a_cte_derives() {
    for (join, expected) in joins("x") {
        let merged = format!("select k from s {join} join u using (k)");
        for sql in [
            format!("with c as ({merged}) select upper(k) as x from c"),
            format!("select upper(k) as x from ({merged}) d"),
        ] {
            assert_eq!(trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], &sql), expected, "{sql}");
        }
    }
}

#[test]
fn a_key_transformed_where_it_is_merged_keeps_its_sides() {
    for (join, expected) in joins("j") {
        for computed in ["upper(k) j", "cast(k as varchar) as j"] {
            let outermost = format!("select {computed} from s {join} join u using (k)");
            let cte = format!("with f as ({outermost}) select j from f");
            for sql in [outermost, cte] {
                assert_eq!(trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], &sql), expected, "{sql}");
            }
        }
    }
    // The key's sides alone: a column of the right side that the expression reads derives.
    let sql = "select concat(k, c) as j from s left join u using (k)";
    let expected = ["s.k -> t.j derive", "u.c -> t.j derive", "u.k -> t.j rows"];
    assert_eq!(trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], sql), expected);
}
