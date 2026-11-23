// SPDX-License-Identifier: EUPL-1.2
//! The clauses of the proof of concept's catalogue that its Snowflake cases leave out,
//! as the Edge entry of `docs/concepts.md` and its `definition.md` give them: uses in
//! subqueries, filtering branches, keys that a join merges, an aggregate's order.

use serde_json::json;

use super::{outline, request, respond, trace};

/// The answer to `select k, a, c from s <join> join u using (k)`.
fn using(join: &str) -> Vec<String> {
    trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], &format!("select k, a, c from s {join} join u using (k)"))
}

/// The edges of `using`'s `a` and `c`, which the join's keys decide the rows of.
const USING_ROWS: [&str; 6] =
    ["s.a -> t.a copy", "s.k -> t.a rows", "u.k -> t.a rows", "s.k -> t.c rows", "u.c -> t.c copy", "u.k -> t.c rows"];

#[test]
fn a_where_subquery_decides_rows_of_every_column() {
    let found = trace(
        &[("s", &["a", "b"]), ("u", &["x", "c"])],
        "select a, b from s where a > (select max(x) from u where c > 0)",
    );
    let expected = [
        "s.a -> t.a copy",
        "u.c -> t.a rows",
        "u.x -> t.a rows",
        "s.a -> t.b rows",
        "s.b -> t.b copy",
        "u.c -> t.b rows",
        "u.x -> t.b rows",
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_scalar_subquery_reaches_only_its_column() {
    let found = trace(&[("s", &["a"]), ("u", &["x", "c"])], "select a, (select max(x) from u where c > 0) as m from s");
    assert_eq!(found, ["s.a -> t.a copy", "u.c -> t.m rows", "u.x -> t.m derive"]);
}

#[test]
fn a_scalar_subquery_after_a_star_is_a_gap() {
    let found = trace(&[("s", &["a"]), ("u", &["x", "c"])], "select *, (select max(x) from u where c > 0) as m from s");
    assert_eq!(found, ["s.a -> t.a copy", "u.x -> t.m derive", "gap unattributed_use - -"]);
}

#[test]
fn a_scalar_subquery_in_a_cte_reaches_the_columns_that_read_it() {
    let sql = "with c as (select a, (select max(x) from u where u.k = s.k) as m from s) select m as top, a from c";
    let found = trace(&[("s", &["k", "a"]), ("u", &["k", "x"])], sql);
    assert_eq!(found, ["s.k -> t.top rows", "u.k -> t.top rows", "u.x -> t.top derive", "s.a -> t.a copy"]);
}

#[test]
fn a_nested_subquery_reaches_what_its_holder_reaches() {
    let sql = "select (select max(x) from u where u.k in (select k from v where v.z = s.a)) as m, b from s";
    let found = trace(&[("s", &["a", "b"]), ("u", &["k", "x"]), ("v", &["k", "z"])], sql);
    let expected = [
        "s.a -> t.m rows",
        "u.k -> t.m rows",
        "u.x -> t.m derive",
        "v.k -> t.m rows",
        "v.z -> t.m rows",
        "s.b -> t.b copy",
    ];
    assert_eq!(found, expected);
}

#[test]
fn in_decides_rows() {
    for in_ in ["in", "not in"] {
        let sql = format!("select a from s where k {in_} (select k from u where c > 0)");
        let found = trace(&[("s", &["a", "k"]), ("u", &["k", "c"])], &sql);
        assert_eq!(found, ["s.a -> t.a copy", "s.k -> t.a rows", "u.c -> t.a rows", "u.k -> t.a rows"], "{in_}");
    }
}

#[test]
fn exists_decides_rows() {
    for exists in ["exists", "not exists"] {
        let sql = format!("select a from s where {exists} (select 1 from u where u.k = s.k)");
        let found = trace(&[("s", &["a", "k"]), ("u", &["k"])], &sql);
        assert_eq!(found, ["s.a -> t.a copy", "s.k -> t.a rows", "u.k -> t.a rows"], "{exists}");
    }
}

#[test]
fn any_and_all_decide_rows() {
    for compared in ["b = any", "b > all"] {
        let sql = format!("select a from s where {compared} (select x from u where c > 0)");
        let found = trace(&[("s", &["a", "b"]), ("u", &["x", "c"])], &sql);
        assert_eq!(found, ["s.a -> t.a copy", "s.b -> t.a rows", "u.c -> t.a rows", "u.x -> t.a rows"], "{compared}");
    }
}

#[test]
fn a_correlated_column_decides_rows() {
    let found =
        trace(&[("s", &["k"]), ("u", &["k", "x"])], "select k, (select max(x) from u where u.k = s.k) as m from s");
    assert_eq!(found, ["s.k -> t.k copy", "s.k -> t.m rows", "u.k -> t.m rows", "u.x -> t.m derive"]);
}

#[test]
fn an_outer_column_in_a_scalar_select_list_is_a_gap() {
    let sql = "select a, (select s.b + max(x) from u where u.k = s.k) as m from s";
    let found = trace(&[("s", &["a", "b", "k"]), ("u", &["k", "x"])], sql);
    let expected =
        ["s.a -> t.a copy", "s.k -> t.m rows", "u.k -> t.m rows", "u.x -> t.m derive", "gap unresolved_reference - m"];
    assert_eq!(found, expected);
}

#[test]
fn a_tuple_in_is_a_gap() {
    let found =
        trace(&[("s", &["k", "a", "b"]), ("u", &["x", "c"])], "select k from s where (a, b) in (select x, c from u)");
    assert_eq!(found, ["s.k -> t.k copy", "gap unattributed_use - -"]);
}

#[test]
fn the_filter_branch_of_intersect_decides_rows() {
    let found = trace(&[("s", &["a"]), ("u", &["x"])], "select a from s intersect select x from u");
    assert_eq!(found, ["s.a -> t.a copy", "u.x -> t.a rows"]);
}

#[test]
fn the_filter_branch_of_except_decides_rows() {
    let found = trace(&[("s", &["a", "b"]), ("u", &["x", "c"])], "select a, b from s except select x, c from u");
    let expected = [
        "s.a -> t.a copy",
        "u.c -> t.a rows",
        "u.x -> t.a rows",
        "s.b -> t.b copy",
        "u.c -> t.b rows",
        "u.x -> t.b rows",
    ];
    assert_eq!(found, expected);
}

#[test]
fn minus_is_except() {
    assert_eq!(
        trace(&[("s", &["a"]), ("u", &["x"])], "select a from s minus select x from u"),
        ["s.a -> t.a copy", "u.x -> t.a rows"]
    );
}

#[test]
fn a_filter_branch_in_a_cte_decides_rows() {
    let sql = "with c as (select k from s intersect select k from u) select k from c";
    assert_eq!(trace(&[("s", &["k"]), ("u", &["k"])], sql), ["s.k -> t.k copy", "u.k -> t.k rows"]);
}

#[test]
fn a_filter_branch_in_a_chain_decides_rows() {
    let chains = [
        (
            "select a from s union select x from u except select z from v",
            ["s.a -> t.a copy", "u.x -> t.a copy", "v.z -> t.a rows"],
        ),
        (
            "select a from s except select x from u union all select z from v",
            ["s.a -> t.a copy", "u.x -> t.a rows", "v.z -> t.a copy"],
        ),
    ];
    for (sql, expected) in chains {
        assert_eq!(trace(&[("s", &["a"]), ("u", &["x"]), ("v", &["z"])], sql), expected, "{sql}");
    }
}

#[test]
fn an_inner_using_key_copies_both() {
    assert_eq!(using("inner"), [&["s.k -> t.k copy", "u.k -> t.k copy"][..], &USING_ROWS].concat());
}

#[test]
fn a_left_using_key_copies_the_left_and_the_right_decides_rows() {
    assert_eq!(using("left"), [&["s.k -> t.k copy", "u.k -> t.k rows"][..], &USING_ROWS].concat());
}

#[test]
fn a_right_using_key_mirrors_the_left() {
    assert_eq!(using("right"), [&["s.k -> t.k rows", "u.k -> t.k copy"][..], &USING_ROWS].concat());
}

#[test]
fn a_full_using_key_derives_from_both() {
    assert_eq!(using("full"), [&["s.k -> t.k derive", "u.k -> t.k derive"][..], &USING_ROWS].concat());
}

#[test]
fn a_using_key_over_ctes_finds_its_sides() {
    let sql = "with o as (select k as key, a from s), c as (select k as key, x from u) \
               select key, a, x from o left join c using (key)";
    let expected = [
        "s.k -> t.key copy",
        "u.k -> t.key rows",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.k -> t.a rows",
        "s.k -> t.x rows",
        "u.k -> t.x rows",
        "u.x -> t.x copy",
    ];
    assert_eq!(trace(&[("s", &["k", "a"]), ("u", &["k", "x"])], sql), expected);
}

#[test]
fn a_natural_left_key_copies_the_left() {
    let found = trace(&[("s", &["k", "a", "b"]), ("w", &["k", "a", "y"])], "select k, a from s natural left join w");
    let expected = [
        "s.a -> t.k rows",
        "s.k -> t.k copy",
        "w.a -> t.k rows",
        "w.k -> t.k rows",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "w.a -> t.a rows",
        "w.k -> t.a rows",
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_key_merged_across_three_sources_is_a_gap() {
    let sql = "select k from s join u using (k) join v using (k)";
    let found = trace(&[("s", &["k"]), ("u", &["k"]), ("v", &["k"])], sql);
    assert_eq!(found, ["s.k -> t.k rows", "u.k -> t.k rows", "v.k -> t.k rows", "gap unattributed_use - k"]);
}

#[test]
fn an_aggregate_order_derives() {
    let mut request =
        request(&[("s", &["k", "a", "b"])], &[("t", "select k, string_agg(a, ',' order by b) as l from s group by k")]);
    request["dialect"] = json!("duckdb");
    assert_eq!(
        outline(&respond(&request)[0]),
        ["s.k -> t.k copy", "s.a -> t.l derive", "s.b -> t.l derive", "s.k -> t.l rows"]
    );
}

#[test]
fn within_group_derives() {
    let sql = "select k, listagg(a, ',') within group (order by b) as l from s group by k";
    assert_eq!(
        trace(&[("s", &["k", "a", "b"])], sql),
        ["s.k -> t.k copy", "s.a -> t.l derive", "s.b -> t.l derive", "s.k -> t.l rows"]
    );
}
