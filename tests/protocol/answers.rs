// SPDX-License-Identifier: EUPL-1.2
//! The parts of an answer: its columns, its refusals and its gaps, as
//! `docs/protocol.md` and the Answer entry of `docs/concepts.md` define them, and the
//! names of T-SQL, which compare as SQL Server's default collation compares them,
//! ignoring case, bare or delimited.

use serde_json::{Value, json};

use super::{answers, assert_refused, outline, request, respond, trace};

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

/// A request in T-SQL of `catalog` and `writes`, as [`request`] builds one.
fn tsql(catalog: &[(&str, &[&str])], writes: &[(&str, &str)]) -> Value {
    let mut request = request(catalog, writes);
    request["dialect"] = json!("tsql");
    request
}

/// The outline of the answer to `sql` against `catalog` in T-SQL, its target `t`.
fn traced(catalog: &[(&str, &[&str])], sql: &str) -> Vec<String> {
    outline(&respond(&tsql(catalog, &[("t", sql)]))[0])
}

#[test]
fn tsql_names_ignore_case_delimited_or_not() {
    let cases = [
        ("OrderId", "[OrderId]"),
        ("OrderId", "ORDERID"),
        ("OrderId", "\"orderID\""),
        ("[OrderId]", "orderid"),
        ("[OrderId]", "[ORDERID]"),
        ("\"orderid\"", "[OrderID]"),
    ];
    for (declared, read) in cases {
        let found = traced(&[("s", &[declared])], &format!("select {read} as x from s"));
        assert_eq!(found, [format!("s.{declared} -> t.x copy")], "{declared} read as {read}");
    }
}

#[test]
fn tsql_relations_ignore_case_delimited_or_not() {
    for relation in ["dbo.customer", "[DBO].[CUSTOMER]", "\"dbo\".\"customer\"", "Dbo.[customer]"] {
        let found = traced(&[("[dbo].[Customer]", &["Id"])], &format!("select id from {relation}"));
        assert_eq!(found, ["[dbo].[Customer].Id -> t.id copy"], "{relation}");
    }
    let mut request = tsql(&[("[Sales].[dbo].[Customer]", &["Id"])], &[("t", "select id from CUSTOMER")]);
    request["defaults"] = json!({"database": "sales", "schema": "[DBO]"});
    assert_eq!(outline(&respond(&request)[0]), ["[Sales].[dbo].[Customer].Id -> t.id copy"]);
}

#[test]
fn tsql_a_view_read_by_another_spelling_is_known() {
    let writes = [
        ("[dbo].[vOrders]", "create view dbo.vorders as select [OrderId], amount as [Total] from dbo.orders"),
        ("u", "select orderid, TOTAL from DBO.VORDERS"),
    ];
    let found: Vec<Vec<String>> =
        respond(&tsql(&[("[dbo].[Orders]", &["[OrderId]", "[Amount]"])], &writes)).iter().map(outline).collect();
    let view = [
        "[dbo].[Orders].[OrderId] -> [dbo].[vOrders].[OrderId] copy",
        "[dbo].[Orders].[Amount] -> [dbo].[vOrders].[Total] copy",
    ];
    let reader = ["[dbo].[vOrders].[OrderId] -> u.orderid copy", "[dbo].[vOrders].[Total] -> u.TOTAL copy"];
    assert_eq!(found, [view.to_vec(), reader.to_vec()]);
}

#[test]
fn tsql_declarations_alike_but_for_case_are_refused() {
    let relations = tsql(&[("[dbo].[T]", &["a"]), ("dbo.t", &["b"])], &[("u", "select a from dbo.t")]);
    assert_refused(&answers(relations.to_string().as_bytes()), "dbo.t twice");
    let columns = tsql(&[("[dbo].[T]", &["[A]", "a"])], &[("u", "select a from dbo.t")]);
    assert_refused(&answers(columns.to_string().as_bytes()), "a twice");
}

#[test]
fn tsql_a_string_alias_is_a_name() {
    for (sql, child) in [
        ("with c as (select a as 'Total' from s) select total from c", "total"),
        ("with c as (select a 'Total' from s) select TOTAL from c", "TOTAL"),
        ("select x.total from (select a as 'Total' from s) as x", "total"),
    ] {
        assert_eq!(traced(&[("s", &["a"])], sql), [format!("s.a -> t.{child} copy")], "{sql}");
    }
}

#[test]
fn tsql_a_table_alias_of_another_case_is_resolved() {
    for sql in [
        "select o.a, O.b from s as O",
        "select [X].a, x.b from s as x",
        "select q.a, Q.b from (select a, b from s) as [Q]",
    ] {
        assert_eq!(traced(&[("s", &["a", "b"])], sql), ["s.a -> t.a copy", "s.b -> t.b copy"], "{sql}");
    }
}

#[test]
fn tsql_a_cte_named_in_another_case_is_read() {
    let catalog: &[(&str, &[&str])] = &[("s", &["a", "b"])];
    let literal = &respond(&tsql(catalog, &[("t", "with [Été] as (select 1 as x) select x from été")]))[0];
    assert_eq!(literal["columns"], json!(["x"]));
    assert!(outline(literal).is_empty(), "{literal}");
    let starred = "with [Été] as (select a from s) select [été].* from [Été]";
    assert_eq!(traced(catalog, starred), ["s.a -> t.a copy"]);
    let filtered = "with [Été] as (select a from s where b > 0) select a from été";
    assert_eq!(traced(catalog, filtered), ["s.a -> t.a copy", "s.b -> t.a rows"]);
}

#[test]
fn tsql_edges_keep_their_spellings() {
    let catalog: &[(&str, &[&str])] = &[("[dbo].[Orders]", &["[OrderId]", "Amount"])];
    let writes = [("t", "select ORDERID as [Id], [amount] from DBO.ORDERS"), ("u", "select * from dbo.orders")];
    let found = respond(&tsql(catalog, &writes));
    assert_eq!(found[0]["columns"], json!(["[Id]", "[amount]"]));
    assert_eq!(
        outline(&found[0]),
        ["[dbo].[Orders].[OrderId] -> t.[Id] copy", "[dbo].[Orders].Amount -> t.[amount] copy"]
    );
    assert_eq!(found[1]["columns"], json!(["[OrderId]", "Amount"]));
    assert_eq!(
        outline(&found[1]),
        ["[dbo].[Orders].[OrderId] -> u.[OrderId] copy", "[dbo].[Orders].Amount -> u.Amount copy"]
    );
}
