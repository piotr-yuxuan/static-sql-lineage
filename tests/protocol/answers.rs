// SPDX-License-Identifier: EUPL-1.2
//! The parts of an answer: its columns, its refusals and its gaps, as
//! `docs/protocol.md` and the Answer entry of `docs/concepts.md` define them, and the
//! difference that `docs/differences.md` records for T-SQL.

use serde_json::json;

use super::{outline, request, respond, trace};

#[test]
fn columns_name_a_write_with_no_edge() {
    let answer = &respond(&request(&[("s", &["a"])], &[("t", "select count(*) as n from s")]))[0];
    assert_eq!(answer["columns"], json!(["n"]));
    assert!(outline(answer).is_empty(), "{answer}");
}

#[test]
fn sql_that_does_not_parse_is_refused() {
    for sql in ["select from where", "select a from s; select a from s"] {
        assert_eq!(trace(&[("s", &["a"])], sql), ["refused unparsed"], "{sql}");
    }
}

#[test]
fn relations_alike_but_for_case_are_ambiguous() {
    let sql = "select o.x as p, p.y as q from \"Orders\" o join orders p on o.x = p.y";
    assert_eq!(trace(&[("\"Orders\"", &["x"]), ("orders", &["y"])], sql), ["gap ambiguous_relation - -"]);
}

#[test]
fn a_column_reported_as_two_declared_is_unresolved() {
    assert_eq!(trace(&[("w", &["\"a\"", "A"])], "select \"a\" as x from w"), ["gap unresolved_reference - x"]);
}

#[test]
fn an_unsure_reference_is_unresolved() {
    assert_eq!(trace(&[("s", &["a"]), ("u", &["a"])], "select a from s, u"), ["gap unresolved_reference - a"]);
}

#[test]
fn an_unknown_star_makes_its_target_unknown() {
    let answers = respond(&request(&[], &[("u", "select * from r"), ("v", "select a from u")]));
    assert_eq!(outline(&answers[1]), ["gap unknown_relation u -"]);
}

#[test]
fn a_quoted_relation_is_completed_by_defaults() {
    let mut request = request(&[("db.s.\"Orders\"", &["id"])], &[("t", "select id from \"Orders\"")]);
    request["defaults"] = json!({"database": "db", "schema": "s"});
    assert_eq!(outline(&respond(&request)[0]), ["db.s.\"Orders\".id -> t.id copy"]);
}

#[test]
fn tsql_brackets_keep_their_case() {
    let mut request =
        request(&[("s", &["OrderId"])], &[("t", "select [OrderId] as x from s"), ("u", "select ORDERID as x from s")]);
    request["dialect"] = json!("tsql");
    let answers = respond(&request);
    assert_eq!(outline(&answers[0]), ["refused invalid"]);
    assert_eq!(outline(&answers[1]), ["s.OrderId -> u.x copy"]);
}
