// SPDX-License-Identifier: EUPL-1.2
//! Output columns through a `*`, each a `copy` as the Edge entry of `docs/concepts.md`
//! says, and named as what the `*` reads names it, as its Relation entry says.

use serde_json::{Value, json};

use super::{outline, request, respond, trace};

/// The output columns of the answer to `sql` against `catalog`, its target `t`.
fn columns(catalog: &[(&str, &[&str])], sql: &str) -> Value {
    respond(&request(catalog, &[("t", sql)]))[0]["columns"].clone()
}

#[test]
fn a_star_copies_each_column() {
    assert_eq!(trace(&[("s", &["a", "b"])], "select * from s"), ["s.a -> t.a copy", "s.b -> t.b copy"]);
    assert_eq!(columns(&[("s", &["a", "b"])], "select * from s"), json!(["a", "b"]));
}

#[test]
fn a_qualified_star_reads_its_relation() {
    let found = trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], "select x.* from s as x join u on x.k = u.k");
    assert_eq!(found, ["s.k -> t.k copy", "u.k -> t.k rows", "s.a -> t.a copy", "s.k -> t.a rows", "u.k -> t.a rows"]);
}

#[test]
fn a_star_over_a_derived_table_keeps_its_quoting() {
    let sql = "select * from (select a as \"X\" from s) d";
    assert_eq!(columns(&[("s", &["a"])], sql), json!(["\"X\""]));
    assert_eq!(trace(&[("s", &["a"])], sql), ["s.a -> t.\"X\" copy"]);
}

#[test]
fn exclude_drops_a_column() {
    assert_eq!(trace(&[("s", &["a", "b"])], "select * exclude (B) from s"), ["s.a -> t.a copy"]);
}

#[test]
fn rename_renames_a_column() {
    let found = trace(&[("s", &["a", "b"])], "select * rename (a as \"Aa\") from s");
    assert_eq!(found, ["s.a -> t.\"Aa\" copy", "s.b -> t.b copy"]);
}

#[test]
fn replace_keeps_the_name() {
    let found = trace(&[("s", &["a", "b"])], "select * replace (a + 1 as b) from s");
    assert_eq!(found, ["s.a -> t.a copy", "s.a -> t.b derive"]);
}

#[test]
fn a_star_over_a_join_lists_both_relations() {
    let (catalog, sql): (&[(&str, &[&str])], _) =
        (&[("s", &["k", "a"]), ("u", &["j", "c"])], "select * from s join u on s.k = u.j");
    assert_eq!(columns(catalog, sql), json!(["k", "a", "j", "c"]));
    let expected = [
        "s.k -> t.k copy",
        "u.j -> t.k rows",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.j -> t.a rows",
        "s.k -> t.j rows",
        "u.j -> t.j copy",
        "s.k -> t.c rows",
        "u.c -> t.c copy",
        "u.j -> t.c rows",
    ];
    assert_eq!(trace(catalog, sql), expected);
}

/// Two relations with the columns `k` and `a` in common, and `w2` with `w`'s columns in
/// another order.
const COMMON: &[(&str, &[&str])] = &[("s", &["k", "a", "b"]), ("w", &["k", "a", "y"]), ("w2", &["y", "a", "k"])];

/// The output columns of the answer to `sql` against `COMMON` in `dialect`.
fn columns_in(dialect: &str, sql: &str) -> Value {
    let mut request = request(COMMON, &[("t", sql)]);
    request["dialect"] = json!(dialect);
    respond(&request)[0]["columns"].clone()
}

#[test]
fn using_keys_come_first_in_snowflake() {
    assert_eq!(columns_in("snowflake", "select * from s join w using (a, k)"), json!(["a", "k", "b", "y"]));
    assert_eq!(columns_in("snowflake", "select * from w2 join s using (k, a)"), json!(["k", "a", "y", "b"]));
}

#[test]
fn using_keys_stay_in_place_in_duckdb() {
    assert_eq!(columns_in("duckdb", "select * from s join w using (a, k)"), json!(["k", "a", "b", "y"]));
    assert_eq!(columns_in("duckdb", "select * from w2 join s using (k, a)"), json!(["y", "a", "k", "b"]));
}

#[test]
fn natural_join_merges_common_columns() {
    assert_eq!(columns_in("snowflake", "select * from s natural join w"), json!(["k", "a", "b", "y"]));
    assert_eq!(columns_in("snowflake", "select * from w2 natural join s"), json!(["a", "k", "y", "b"]));
}

#[test]
fn a_star_with_a_duplicate_name_is_refused() {
    let catalog: &[(&str, &[&str])] = &[("s", &["k", "a", "b"]), ("w", &["k", "a", "y"]), ("v", &["k", "z"])];
    for sql in ["select * from s join v on s.k = v.k", "select * from s join w using (k) join v using (k)"] {
        assert_eq!(trace(catalog, sql), ["refused duplicate_column"], "{sql}");
    }
}

#[test]
fn a_star_in_polyglots_other_order_is_matched_by_name() {
    let (catalog, sql): (&[(&str, &[&str])], _) =
        (&[("s", &["a", "k"]), ("u", &["k", "c"])], "select * from s join u using (k)");
    assert_eq!(columns(catalog, sql), json!(["k", "a", "c"]));
    let expected = [
        "s.k -> t.k copy",
        "u.k -> t.k copy",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.k -> t.a rows",
        "s.k -> t.c rows",
        "u.c -> t.c copy",
        "u.k -> t.c rows",
    ];
    assert_eq!(trace(catalog, sql), expected);
}

#[test]
fn a_star_over_a_pivot_is_a_wildcard_gap() {
    let sql = "select * from (select region, quarter, amount from sales) pivot (sum(amount) for quarter in ('Q1', 'Q2')) as p";
    let catalog: &[(&str, &[&str])] = &[("sales", &["region", "quarter", "amount"])];
    assert_eq!(columns(catalog, sql), Value::Null);
    assert_eq!(trace(catalog, sql), ["gap wildcard - -"]);
}

#[test]
fn a_column_after_a_comment_is_named() {
    let catalog: &[(&str, &[&str])] = &[("s", &["a", "b"])];
    let list = "select a,\n  -- the second\n  b\nfrom s";
    assert_eq!(trace(catalog, list), ["s.a -> t.a copy", "s.b -> t.b copy"]);
    let star = format!("with c as ({list}) select * from c");
    assert_eq!(trace(catalog, &star), ["s.a -> t.a copy", "s.b -> t.b copy"]);
}

#[test]
fn an_unnamed_output_through_a_star_is_a_wildcard_gap() {
    assert_eq!(trace(&[("s", &["a"])], "with c as (select a + 1 from s) select * from c"), ["gap wildcard - -"]);
}

#[test]
fn a_star_over_a_cte_with_a_column_list_is_a_wildcard_gap() {
    let firsts = [
        "with c (x) as (select a from s) select * from c",
        "select * from (select a from s) as d (x)",
        "select * from s as d (x)",
    ];
    for first in firsts {
        let answers = respond(&request(&[("s", &["a"])], &[("u", first), ("v", "select a as z from u")]));
        assert_eq!(answers[0]["columns"], Value::Null, "{first}");
        assert_eq!(outline(&answers[0]), ["gap wildcard - -"], "{first}");
        assert_eq!(outline(&answers[1]), ["gap unknown_relation u -"], "{first}");
    }
}
