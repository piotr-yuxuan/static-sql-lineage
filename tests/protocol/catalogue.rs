// SPDX-License-Identifier: EUPL-1.2
//! The clauses of the proof of concept's catalogue that its Snowflake cases leave out,
//! as the Edge entry of `docs/concepts.md` and its `definition.md` give them: uses in
//! subqueries.

use super::trace;

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
