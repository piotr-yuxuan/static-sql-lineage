// SPDX-License-Identifier: EUPL-1.2
//! The scenarios of `docs/concepts.md`, each test named after its identifier, with the
//! defaults of its section Scenarios: Snowflake, one write whose target is `t`, and a
//! catalog of what the query names.

use serde_json::{Value, json};

use super::{answers, assert_refused, outline, request, respond, trace};

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
    let request = request(&[("s", &["a"]), ("t", &["a"])], &[("t", "select nope from s"), ("u", "select a from t")]);
    respond(&request).iter().map(outline).collect()
}

/// The request of `dialect.not-built`.
fn bigquery() -> Value {
    let mut request = request(&[("s", &["a"])], &[("t", "select a from s")]);
    request["dialect"] = json!("bigquery");
    request
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
