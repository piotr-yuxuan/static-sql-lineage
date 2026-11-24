// SPDX-License-Identifier: EUPL-1.2
//! Output columns through a `*`, each a `copy` as the Edge entry of `docs/concepts.md`
//! says, and named as what the `*` reads names it, as its Relation entry says.

use serde_json::{Value, json};

use super::{answered, outline, request, request_in, respond, traces};

/// Two relations with the columns `k` and `a` in common, and `w2` with `w`'s columns in
/// another order.
const COMMON: &[(&str, &[&str])] = &[("s", &["k", "a", "b"]), ("w", &["k", "a", "y"]), ("w2", &["y", "a", "k"])];

/// The output columns of the answer to `sql` against `COMMON` in `dialect`.
fn columns_in(dialect: &str, sql: &str) -> Value {
    respond(&request_in(dialect, COMMON, &[("t", sql)]))[0]["columns"].clone()
}

cases! {
    a_star_copies_each_column:
        answered(&[("s", &["a", "b"])], "select * from s", json!(["a", "b"]), &["s.a -> t.a copy", "s.b -> t.b copy"]);
    a_qualified_star_reads_its_relation: traces(&[("s", &["k", "a"]), ("u", &["k", "c"])],
        "select x.* from s as x join u on x.k = u.k",
        &["s.k -> t.k copy", "u.k -> t.k rows", "s.a -> t.a copy", "s.k -> t.a rows", "u.k -> t.a rows"]);
    a_star_over_a_derived_table_keeps_its_quoting: answered(&[("s", &["a"])],
        "select * from (select a as \"X\" from s) d", json!(["\"X\""]), &["s.a -> t.\"X\" copy"]);
    exclude_drops_a_column: traces(&[("s", &["a", "b"])], "select * exclude (B) from s", &["s.a -> t.a copy"]);
    rename_renames_a_column: traces(&[("s", &["a", "b"])], "select * rename (a as \"Aa\") from s",
        &["s.a -> t.\"Aa\" copy", "s.b -> t.b copy"]);
    replace_keeps_the_name: traces(&[("s", &["a", "b"])], "select * replace (a + 1 as b) from s",
        &["s.a -> t.a copy", "s.a -> t.b derive"]);
    a_star_over_a_join_lists_both_relations: answered(&[("s", &["k", "a"]), ("u", &["j", "c"])],
        "select * from s join u on s.k = u.j", json!(["k", "a", "j", "c"]), &[
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
    ]);
    using_keys_come_first: for dialect in ["snowflake", "postgresql", "redshift", "databricks"] {
        assert_eq!(columns_in(dialect, "select * from s join w using (a, k)"), json!(["a", "k", "b", "y"]), "{dialect}");
        assert_eq!(columns_in(dialect, "select * from w2 join s using (k, a)"), json!(["k", "a", "y", "b"]), "{dialect}");
    };
    using_keys_stay_in_place_in_duckdb: {
        assert_eq!(columns_in("duckdb", "select * from s join w using (a, k)"), json!(["k", "a", "b", "y"]));
        assert_eq!(columns_in("duckdb", "select * from w2 join s using (k, a)"), json!(["y", "a", "k", "b"]));
    };
    natural_join_merges_common_columns: {
        assert_eq!(columns_in("snowflake", "select * from s natural join w"), json!(["k", "a", "b", "y"]));
        assert_eq!(columns_in("snowflake", "select * from w2 natural join s"), json!(["a", "k", "y", "b"]));
    };
    a_star_with_a_duplicate_name_is_refused: {
        let catalog: &[(&str, &[&str])] = &[("s", &["k", "a", "b"]), ("w", &["k", "a", "y"]), ("v", &["k", "z"])];
        traces(catalog, "select * from s join v on s.k = v.k", &["refused duplicate_column"]);
        traces(catalog, "select * from s join w using (k) join v using (k)", &["refused duplicate_column"]);
    };
    a_star_in_polyglots_other_order_is_matched_by_name: answered(&[("s", &["a", "k"]), ("u", &["k", "c"])],
        "select * from s join u using (k)", json!(["k", "a", "c"]), &[
        "s.k -> t.k copy",
        "u.k -> t.k copy",
        "s.a -> t.a copy",
        "s.k -> t.a rows",
        "u.k -> t.a rows",
        "s.k -> t.c rows",
        "u.c -> t.c copy",
        "u.k -> t.c rows",
    ]);
    a_star_over_a_pivot_is_a_wildcard_gap: answered(&[("sales", &["region", "quarter", "amount"])],
        "select * from (select region, quarter, amount from sales) \
        pivot (sum(amount) for quarter in ('Q1', 'Q2')) as p", Value::Null, &["gap wildcard - -"]);
    a_column_after_a_comment_is_named: {
        let list = "select a,\n  -- the second\n  b\nfrom s";
        traces(&[("s", &["a", "b"])], list, &["s.a -> t.a copy", "s.b -> t.b copy"]);
        let star = format!("with c as ({list}) select * from c");
        traces(&[("s", &["a", "b"])], &star, &["s.a -> t.a copy", "s.b -> t.b copy"]);
    };
    an_unnamed_output_through_a_star_is_a_wildcard_gap:
        traces(&[("s", &["a"])], "with c as (select a + 1 from s) select * from c", &["gap wildcard - -"]);
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
