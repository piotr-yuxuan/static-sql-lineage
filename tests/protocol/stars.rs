// SPDX-License-Identifier: EUPL-1.2
//! Output columns through a `*`, each a `copy` as the Edge entry of `docs/concepts.md`
//! says, and named as what the `*` reads names it, as its Relation entry says.

use super::{outline, request, respond, trace};

/// The output columns of the answer to `sql` against `catalog`, its target `t`.
fn columns(catalog: &[(&str, &[&str])], sql: &str) -> serde_json::Value {
    respond(&request(catalog, &[("t", sql)]))[0]["columns"].clone()
}

#[test]
fn a_star_copies_each_column() {
    assert_eq!(trace(&[("s", &["a", "b"])], "select * from s"), ["s.a -> t.a copy", "s.b -> t.b copy"]);
    assert_eq!(columns(&[("s", &["a", "b"])], "select * from s"), serde_json::json!(["a", "b"]));
}

#[test]
fn a_qualified_star_reads_its_relation() {
    let found = trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], "select x.* from s as x join u on x.k = u.k");
    assert_eq!(found, ["s.k -> t.k copy", "u.k -> t.k rows", "s.a -> t.a copy", "s.k -> t.a rows", "u.k -> t.a rows"]);
}

#[test]
fn a_star_over_a_derived_table_keeps_its_quoting() {
    let sql = "select * from (select a as \"X\" from s) d";
    assert_eq!(columns(&[("s", &["a"])], sql), serde_json::json!(["\"X\""]));
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
fn a_star_over_several_relations_is_a_wildcard_gap() {
    let found = trace(&[("s", &["k", "a"]), ("u", &["k", "c"])], "select * from s join u on s.k = u.k");
    assert_eq!(found, ["gap wildcard - -"]);
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
        assert_eq!(answers[0]["columns"], serde_json::Value::Null, "{first}");
        assert_eq!(outline(&answers[0]), ["gap wildcard - -"], "{first}");
        assert_eq!(outline(&answers[1]), ["gap unknown_relation u -"], "{first}");
    }
}
