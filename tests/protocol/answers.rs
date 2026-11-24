// SPDX-License-Identifier: EUPL-1.2
//! The parts of an answer: its columns, its refusals and its gaps, as
//! `docs/protocol.md` and the Answer entry of `docs/concepts.md` define them, and the
//! names of T-SQL, which compare as SQL Server's default collation compares them,
//! ignoring case, bare or delimited.

use serde_json::json;

use super::{
    answered, answers, assert_refused, outline, outlines, request, request_in, respond, trace_in, traces, traces_in,
};

cases! {
    columns_name_a_write_with_no_edge: answered(&[("s", &["a"])], "select count(*) as n from s", json!(["n"]), &[]);
    sql_that_does_not_parse_is_refused: {
        traces(&[("s", &["a"])], "select from where", &["refused unparsed"]);
        traces(&[("s", &["a"])], "select a from s; select a from s", &["refused unparsed"]);
    };
    relations_alike_but_for_case_are_ambiguous: traces(&[("\"Orders\"", &["x"]), ("orders", &["y"])],
        "select o.x as p, p.y as q from \"Orders\" o join orders p on o.x = p.y", &["gap ambiguous_relation - -"]);
    a_column_reported_as_two_declared_is_unresolved:
        traces(&[("w", &["\"a\"", "A"])], "select \"a\" as x from w", &["gap unresolved_reference - x"]);
    an_unsure_reference_is_unresolved:
        traces(&[("s", &["a"]), ("u", &["a"])], "select a from s, u", &["gap unresolved_reference - a"]);
    an_unknown_star_makes_its_target_unknown: assert_eq!(
        outlines(&request(&[], &[("u", "select * from r"), ("v", "select a from u")]))[1],
        ["gap unknown_relation u -"]
    );
    a_quoted_relation_is_completed_by_defaults: {
        let mut request = request(&[("db.s.\"Orders\"", &["id"])], &[("t", "select id from \"Orders\"")]);
        request["defaults"] = json!({"database": "db", "schema": "s"});
        assert_eq!(outline(&respond(&request)[0]), ["db.s.\"Orders\".id -> t.id copy"]);
    };
    tsql_declarations_alike_but_for_case_are_refused: {
        let catalog: &[(&str, &[&str])] = &[("[dbo].[T]", &["a"]), ("dbo.t", &["b"])];
        let relations = request_in("tsql", catalog, &[("u", "select a from dbo.t")]);
        assert_refused(&answers(relations.to_string().as_bytes()), "dbo.t twice");
        let columns = request_in("tsql", &[("[dbo].[T]", &["[A]", "a"])], &[("u", "select a from dbo.t")]);
        assert_refused(&answers(columns.to_string().as_bytes()), "a twice");
    };
    tsql_a_string_alias_is_a_name: {
        let catalog: &[(&str, &[&str])] = &[("s", &["a"])];
        let (total, capitals) = (["s.a -> t.total copy"], ["s.a -> t.TOTAL copy"]);
        traces_in("tsql", catalog, "with c as (select a as 'Total' from s) select total from c", &total);
        traces_in("tsql", catalog, "with c as (select a 'Total' from s) select TOTAL from c", &capitals);
        traces_in("tsql", catalog, "select x.total from (select a as 'Total' from s) as x", &total);
    };
    tsql_a_table_alias_of_another_case_is_resolved: {
        let catalog: &[(&str, &[&str])] = &[("s", &["a", "b"])];
        let expected = ["s.a -> t.a copy", "s.b -> t.b copy"];
        traces_in("tsql", catalog, "select o.a, O.b from s as O", &expected);
        traces_in("tsql", catalog, "select [X].a, x.b from s as x", &expected);
        traces_in("tsql", catalog, "select q.a, Q.b from (select a, b from s) as [Q]", &expected);
    };
    // A table alias read in another case, or quoted, each quoted as the dialect quotes.
    aliases_ignore_case_in_redshift_and_databricks: {
        let (dialect, q) = ("redshift", '"');
        let catalog: &[(&str, &[&str])] = &[("s", &["a", "b"])];
        let expected = ["s.a -> t.a copy", "s.b -> t.b copy"];
        traces_in(dialect, catalog, "select o.a, O.b from s as O", &expected);
        traces_in(dialect, catalog, &format!("select {q}X{q}.a, x.b from s as x"), &expected);
        traces_in(dialect, catalog, &format!("select q.a, Q.b from (select a, b from s) as {q}Q{q}"), &expected);
        let amount = format!("{q}Amount{q}");
        let catalog: &[(&str, &[&str])] = &[("s", &[amount.as_str()]), ("u", &[amount.as_str()])];
        let cte = format!("with c as (select {amount} from s intersect select {amount} from u)");
        let expected = [format!("s.{amount} -> t.amount copy"), format!("u.{amount} -> t.amount rows")];
        assert_eq!(trace_in(dialect, catalog, &format!("{cte} select x.amount from c as {q}X{q}")), expected);
    };
    tsql_a_filter_branch_in_a_cte_reads_bracketed_names: {
        // polyglot-sql's lineage of the piece's own text does not follow `x` to `[X]`.
        let catalog: &[(&str, &[&str])] = &[("s", &["[Amount]"]), ("u", &["[Amount]"])];
        let cte = "with c as (select [Amount] from s intersect select [Amount] from u)";
        let expected = ["s.[Amount] -> t.amount copy", "u.[Amount] -> t.amount rows"];
        traces_in("tsql", catalog, &format!("{cte} select amount from c"), &expected);
        traces_in("tsql", catalog, &format!("{cte} select x.amount from c as [X]"), &expected);
    };
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
        let found = trace_in("tsql", &[("s", &[declared])], &format!("select {read} as x from s"));
        assert_eq!(found, [format!("s.{declared} -> t.x copy")], "{declared} read as {read}");
    }
}

#[test]
fn tsql_relations_ignore_case_delimited_or_not() {
    for relation in ["dbo.customer", "[DBO].[CUSTOMER]", "\"dbo\".\"customer\"", "Dbo.[customer]"] {
        let found = trace_in("tsql", &[("[dbo].[Customer]", &["Id"])], &format!("select id from {relation}"));
        assert_eq!(found, ["[dbo].[Customer].Id -> t.id copy"], "{relation}");
    }
    let mut request = request_in("tsql", &[("[Sales].[dbo].[Customer]", &["Id"])], &[("t", "select id from CUSTOMER")]);
    request["defaults"] = json!({"database": "sales", "schema": "[DBO]"});
    assert_eq!(outline(&respond(&request)[0]), ["[Sales].[dbo].[Customer].Id -> t.id copy"]);
}

#[test]
fn tsql_a_view_read_by_another_spelling_is_known() {
    let writes = [
        ("[dbo].[vOrders]", "create view dbo.vorders as select [OrderId], amount as [Total] from dbo.orders"),
        ("u", "select orderid, TOTAL from DBO.VORDERS"),
    ];
    let found = outlines(&request_in("tsql", &[("[dbo].[Orders]", &["[OrderId]", "[Amount]"])], &writes));
    let view = [
        "[dbo].[Orders].[OrderId] -> [dbo].[vOrders].[OrderId] copy",
        "[dbo].[Orders].[Amount] -> [dbo].[vOrders].[Total] copy",
    ];
    let reader = ["[dbo].[vOrders].[OrderId] -> u.orderid copy", "[dbo].[vOrders].[Total] -> u.TOTAL copy"];
    assert_eq!(found, [view.to_vec(), reader.to_vec()]);
}

#[test]
fn tsql_a_cte_named_in_another_case_is_read() {
    let catalog: &[(&str, &[&str])] = &[("s", &["a", "b"])];
    let literal =
        &respond(&request_in("tsql", catalog, &[("t", "with [Été] as (select 1 as x) select x from été")]))[0];
    assert_eq!(literal["columns"], json!(["x"]));
    assert!(outline(literal).is_empty(), "{literal}");
    let starred = "with [Été] as (select a from s) select [été].* from [Été]";
    assert_eq!(trace_in("tsql", catalog, starred), ["s.a -> t.a copy"]);
    let filtered = "with [Été] as (select a from s where b > 0) select a from été";
    assert_eq!(trace_in("tsql", catalog, filtered), ["s.a -> t.a copy", "s.b -> t.a rows"]);
}

#[test]
fn tsql_edges_keep_their_spellings() {
    let catalog: &[(&str, &[&str])] = &[("[dbo].[Orders]", &["[OrderId]", "Amount"])];
    let writes = [("t", "select ORDERID as [Id], [amount] from DBO.ORDERS"), ("u", "select * from dbo.orders")];
    let found = respond(&request_in("tsql", catalog, &writes));
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
