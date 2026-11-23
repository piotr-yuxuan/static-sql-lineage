// SPDX-License-Identifier: EUPL-1.2
//! The scenarios of `docs/concepts.md`, each test named after its identifier, with the
//! defaults of its section Scenarios: Snowflake, one write whose target is `t`, and a
//! catalog of what the query names.

use serde_json::{Value, json};

use super::{
    answers, assert_refused, lines, outline, outlines, printed_json, request, request_in, respond, trace, trace_in,
};

const CTE: &str = "with orders as (select id from raw.orders) select id from orders";

/// The two answers of `column.defined-quoted` and its reject, `second` the second query.
fn defined(second: &str) -> Vec<String> {
    outline(&respond(&request(&[("s", &["a"])], &[("u", "select a as \"Total\" from s"), ("v", second)]))[1])
}

/// The answer of `relation.completed-by-defaults`.
fn completed() -> Vec<String> {
    let mut request = request(&[("db.s.orders", &["id"])], &[("t", "select id from orders")]);
    request["defaults"] = json!({"database": "db", "schema": "s"});
    outline(&respond(&request)[0])
}

/// The two answers of `answer.refusal-spreads`.
fn spread() -> Vec<Vec<String>> {
    outlines(&request(&[("s", &["a"]), ("t", &["a"])], &[("t", "select nope from s"), ("u", "select a from t")]))
}

/// The request of `dialect.not-built`.
fn bigquery() -> Value {
    request_in("bigquery", &[("s", &["a"])], &[("t", "select a from s")])
}

#[test]
fn relation_cte_is_a_step() {
    assert_eq!(trace(&[("raw.orders", &["id"])], CTE), ["raw.orders.id -> t.id copy"]);
}

#[test]
fn relation_cte_is_an_end() {
    let found = trace(&[("raw.orders", &["id"])], CTE);
    assert!(!found.iter().any(|line| line.starts_with("orders.")), "{found:?}");
}

#[test]
fn column_declared_bare() {
    assert_eq!(trace(&[("s", &["amount"])], "select AMOUNT as a from s"), ["s.amount -> t.a copy"]);
}

#[test]
fn column_declared_quoted() {
    assert_eq!(trace(&[("s", &["\"Amount\""])], "select amount as a from s"), ["refused invalid"]);
}

#[test]
fn column_folded_by_hand() {
    let found = trace(&[("s", &["\"Amount\""])], "select amount as a from s");
    assert!(!found.iter().any(|line| line.starts_with("s.\"Amount\" -> t.a")), "{found:?}");
}

#[test]
fn relation_completed_by_defaults() {
    assert_eq!(completed(), ["db.s.orders.id -> t.id copy"]);
}

#[test]
fn relation_defaults_ignored() {
    let found = completed();
    assert!(!found.contains(&"gap unknown_relation orders -".to_string()), "{found:?}");
}

#[test]
fn column_defined_quoted() {
    assert_eq!(defined("select \"Total\" as x from u"), ["u.\"Total\" -> v.x copy"]);
}

#[test]
fn column_defined_folded() {
    let found = defined("select total as x from u");
    assert!(!found.iter().any(|line| line.starts_with("u.\"Total\" -> v.x")), "{found:?}");
    assert_eq!(found, ["refused invalid"]);
}

#[test]
fn column_defined_through_star() {
    let first = "with c as (select a as \"Total\" from s) select * from c";
    let answers = respond(&request(&[("s", &["a"])], &[("u", first), ("v", "select \"Total\" as x from u")]));
    assert_eq!(outline(&answers[1]), ["u.\"Total\" -> v.x copy"]);
}

/// The two answers of `write.contribute-keeps-columns`.
fn contributed() -> Vec<Vec<String>> {
    let writes = [("t", "insert into t (a) select x from s"), ("u", "create view u as select a, b from t")];
    outlines(&request(&[("t", &["a", "b"]), ("s", &["x"])], &writes))
}

#[test]
fn write_contribute_keeps_columns() {
    assert_eq!(contributed(), [vec!["s.x -> t.a copy"], vec!["t.a -> u.a copy", "t.b -> u.b copy"]]);
}

#[test]
fn write_contribute_as_replace() {
    let found = contributed();
    assert!(!found[1].iter().any(|line| line.starts_with("gap") || line.starts_with("refused")), "{found:?}");
}

/// The answer to `sql` against the catalog of `write.delete-decides-rows`.
fn deleted(sql: &str) -> Vec<String> {
    trace(&[("t", &["a", "b"]), ("s", &["k"])], sql)
}

#[test]
fn write_delete_decides_rows() {
    assert_eq!(deleted("delete from t where a in (select k from s)"), ["s.k -> t.a rows", "s.k -> t.b rows"]);
}

#[test]
fn write_delete_own_condition() {
    let found = deleted("delete from t where b < 0");
    assert!(!found.iter().any(|line| line.contains(" -> ")), "{found:?}");
}

#[test]
fn write_unnamed_output() {
    assert_eq!(trace(&[("s", &["a"])], "select a + 1 from s"), ["refused unnamed_column"]);
}

#[test]
fn write_engine_label() {
    let found = trace(&[("s", &["a"])], "select a + 1 from s");
    assert!(!found.iter().any(|line| line.contains("-> t._col_")), "{found:?}");
}

#[test]
fn write_duplicate_output() {
    assert_eq!(trace(&[("s", &["a"])], "select a, a as A from s"), ["refused duplicate_column"]);
}

#[test]
fn write_quoted_as_duplicate() {
    let found = trace(&[("s", &["a"])], "select a, a as \"a\" from s");
    assert!(!found.iter().any(|line| line.starts_with("refused")), "{found:?}");
}

#[test]
fn edge_alias_is_copy() {
    assert_eq!(trace(&[("s", &["id"])], "select id as order_id from s"), ["s.id -> t.order_id copy"]);
}

#[test]
fn edge_coalesce_is_derive() {
    assert_eq!(trace(&[("s", &["a"])], "select coalesce(a, 0) as a from s"), ["s.a -> t.a derive"]);
}

#[test]
fn edge_order_without_limit() {
    assert_eq!(trace(&[("s", &["a", "b"])], "select a from s order by b"), ["s.a -> t.a copy"]);
}

#[test]
fn edge_order_is_rows() {
    let found = trace(&[("s", &["a", "b"])], "select a from s order by b");
    assert!(!found.iter().any(|line| line.starts_with("s.b ")), "{found:?}");
}

#[test]
fn answer_unknown_star() {
    let found = trace(&[], "select * from r");
    assert_eq!(found[0], "gap unknown_relation r -");
    assert!(matches!(found.as_slice(), [_, wildcard] if wildcard.starts_with("gap wildcard ")), "{found:?}");
}

#[test]
fn answer_silent_none() {
    assert!(!trace(&[], "select * from r").is_empty());
}

#[test]
fn answer_refusal_spreads() {
    assert_eq!(spread(), [vec!["refused invalid"], vec!["gap unknown_relation t -"]]);
}

#[test]
fn answer_stale_shape() {
    let found = spread();
    assert!(!found[1].contains(&"t.a -> u.a copy".to_string()), "{found:?}");
}

/// The second answer of `catalog.refused-contribute-keeps-target`.
fn kept() -> Vec<String> {
    let writes = [("t", "insert into t (nope) select x from s"), ("u", "create view u as select a from t")];
    let answers = respond(&request(&[("t", &["a"]), ("s", &["x"])], &writes));
    assert_eq!(outline(&answers[0]), ["refused invalid"]);
    outline(&answers[1])
}

#[test]
fn catalog_refused_contribute_keeps_target() {
    assert_eq!(kept(), ["t.a -> u.a copy"]);
}

#[test]
fn catalog_refused_contribute_as_unknown() {
    let found = kept();
    assert!(!found.contains(&"gap unknown_relation t -".to_string()), "{found:?}");
}

#[test]
fn catalog_unknown_relation() {
    assert_eq!(trace(&[], "select a from r"), ["gap unknown_relation r -"]);
}

#[test]
fn catalog_columns_guessed() {
    let found = trace(&[], "select a from r");
    assert!(!found.iter().any(|line| line.starts_with("r.a ")), "{found:?}");
}

#[test]
fn dialect_not_built() {
    assert_refused(&answers(bigquery().to_string().as_bytes()), "bigquery");
}

#[test]
fn dialect_generic_fallback() {
    assert!(answers(bigquery().to_string().as_bytes()).stdout.is_empty());
}

/// The answer of `dialect.tsql-ignores-case`.
fn ignoring_case() -> Vec<String> {
    trace_in("tsql", &[("[dbo].[Orders]", &["[OrderId]"])], "select orderid from dbo.orders")
}

#[test]
fn dialect_tsql_ignores_case() {
    assert_eq!(ignoring_case(), ["[dbo].[Orders].[OrderId] -> t.orderid copy"]);
}

#[test]
fn dialect_tsql_delimited_as_exact() {
    let found = ignoring_case();
    assert!(!found.iter().any(|line| line.starts_with("refused")), "{found:?}");
}

/// The script of `catalog.declared-by-create-table`, read by the commands.
const DECLARED: &str = "tests/scripts/snowflake/04-declared.sql";

/// What `trace` gives for `column` in the script of `catalog.declared-by-create-table`.
fn declared(column: &str) -> Value {
    printed_json(&["trace", "--dialect", "snowflake", "--json", column, DECLARED])
}

#[test]
fn catalog_declared_by_create_table() {
    for (column, declared_as) in [("t.a", "a"), ("t.\"B\"", "\"B\"")] {
        let found = declared(column);
        let edge = json!({
            "parent": {"relation": "s", "column": declared_as},
            "child": {"relation": "t", "column": declared_as},
            "kind": "copy",
            "writes": [format!("{DECLARED}:2:1")],
            "depth": 1,
        });
        assert_eq!((&found["edges"], &found["gaps"]), (&json!([edge]), &json!([])), "{column}");
    }
}

#[test]
fn catalog_declaration_as_a_write() {
    let targets = lines(&["graph", "--dialect", "snowflake", DECLARED]);
    assert!(!targets.iter().any(|line| line.split(' ').nth(1) == Some("s")), "{targets:?}");
    let gaps = declared("t.a")["gaps"].clone();
    let unknown = |gap: &Value| gap["reason"] == "unknown_relation" && gap["relation"] == "s";
    assert!(!gaps.as_array().is_some_and(|gaps| gaps.iter().any(unknown)), "{gaps}");
}
