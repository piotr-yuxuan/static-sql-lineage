// SPDX-License-Identifier: EUPL-1.2
//! Writes given as statements, as D1 to D7 of plan 006 and the Write entry of
//! `docs/concepts.md` read them: replace statements, contribute statements, and the
//! statements that are not writes. Expectations from those, the owner's rulings on Q3 and
//! Q4, and the probe's checks on Snowflake.

use serde_json::{Value, json};

use super::{answered, outline, outlines, request_in, respond, trace_in, traces_in};

/// The probe's relations: `s` and `u` read, `t` the target of most writes.
const CATALOG: &[(&str, &[&str])] = &[("s", &["k", "x", "y", "flag"]), ("u", &["k", "z"]), ("t", &["k", "a", "b"])];

/// The request of `writes`, each a target, also its id, and its SQL, in `dialect`
/// against `CATALOG`.
fn writes(dialect: &str, writes: &[(&str, &str)]) -> Value {
    request_in(dialect, CATALOG, writes)
}

/// The outline of each answer to `writes` in `dialect`.
fn traced(dialect: &str, writes: &[(&str, &str)]) -> Vec<Vec<String>> {
    outlines(&self::writes(dialect, writes))
}

/// The outline of the answer to `sql`, its target `t`, in `dialect` against `CATALOG`.
fn trace(dialect: &str, sql: &str) -> Vec<String> {
    trace_in(dialect, CATALOG, sql)
}

/// Asserts that the outline of the answer to `sql`, its target `t`, in `dialect` against
/// `CATALOG` is `expected`.
#[track_caller]
fn expect(dialect: &str, sql: &str, expected: &[&str]) {
    traces_in(dialect, CATALOG, sql, expected);
}

cases! {
    a_view_column_list_renames_its_outputs: {
        answered(CATALOG, "create view t (kk, \"Xx\") as select k, x from s", json!(["kk", "\"Xx\""]),
            &["s.k -> t.kk copy", "s.x -> t.\"Xx\" copy"]);
        expect("tsql", "create view t (kk, [Xx]) as select k, x from s", &["s.k -> t.kk copy", "s.x -> t.[Xx] copy"]);
    };
    a_table_column_list_renames_its_outputs: assert_eq!(
        traced("snowflake", &[
            ("t", "create table t (kk int, \"Aa\" int) as select k, x + 1 from s"),
            ("v", "select \"Aa\" from t"),
        ]),
        [vec!["s.k -> t.kk copy", "s.x -> t.\"Aa\" derive"], vec!["t.\"Aa\" -> v.\"Aa\" copy"]]
    );
    a_column_list_of_another_length_is_refused: {
        expect("snowflake", "create view t (kk) as select k, x from s", &["refused invalid"]);
        expect("snowflake", "create table t (kk int, aa int, bb int) as select k, x from s", &["refused invalid"]);
    };
    statements_that_are_not_writes_are_refused: {
        expect("snowflake", "insert into t (k, a) values (1, 2)", &["refused not_a_query"]);
        expect("snowflake", "create table t (k int, a int)", &["refused not_a_query"]);
        expect("duckdb", "insert into t by name select x as a, k from s", &["refused not_a_query"]);
        expect("snowflake", "insert all into t (k, a) values (k, x) select k, x from s", &["refused not_a_query"]);
        expect("snowflake", "truncate table t", &["refused not_a_query"]);
    };
    a_statement_that_is_not_a_write_keeps_its_target:
        for sql in ["insert into t (k, a) values (1, 2)", "create view v as select k, x from s"] {
            assert_eq!(traced("snowflake", &[("t", sql), ("w", "select b from t")])[1], ["t.b -> w.b copy"], "{sql}");
        };
    insert_maps_its_list_by_position: {
        // Ordered by the target's columns, `k`, `a`, `b`, which are its answer's.
        answered(CATALOG, "insert into t (b, a, k) select x, y + 1, k from s where flag = 1", json!(["k", "a", "b"]), &[
            "s.flag -> t.k rows",
            "s.k -> t.k copy",
            "s.flag -> t.a rows",
            "s.y -> t.a derive",
            "s.flag -> t.b rows",
            "s.x -> t.b copy",
        ]);
        let answer = &respond(&writes("snowflake", &[("t", "insert into t (b, a) select x, y from s")]))[0];
        assert_eq!(answer["columns"], json!(["a", "b"]));
    };
    insert_without_a_list_maps_the_targets_columns: expect("snowflake", "insert into t select k, x, y from s",
        &["s.k -> t.k copy", "s.x -> t.a copy", "s.y -> t.b copy"]);
    an_insert_of_unnamed_outputs_is_read: expect("snowflake", "insert into t select k, x + 1, k from s",
        &["s.k -> t.k copy", "s.x -> t.a derive", "s.k -> t.b copy"]);
    insert_overwrite_keeps_the_targets_columns: assert_eq!(
        traced("snowflake", &[("t", "insert overwrite into t (k, a) select k, x from s"), ("v", "select b from t")]),
        [vec!["s.k -> t.k copy", "s.x -> t.a copy"], vec!["t.b -> v.b copy"]]
    );
    insert_of_another_count_is_refused:
        for sql in ["insert into t (k, a) select k, x, y from s", "insert into t select k, x from s"] {
            expect("snowflake", sql, &["refused invalid"]);
        };
    insert_into_a_column_the_target_lacks_is_refused:
        expect("snowflake", "insert into t (k, nope) select k, x from s", &["refused invalid"]);
    a_with_before_a_statement_is_read: {
        let expected = ["s.flag -> t.k rows", "s.k -> t.k copy", "s.flag -> t.a rows", "s.x -> t.a copy"];
        let cte = "with c as (select k, x from s where flag = 1)";
        expect("snowflake", &format!("{cte} insert into t (k, a) select k, x from c"), &expected);
        expect("snowflake", &format!("insert into t (k, a) {cte} select k, x from c"), &expected);
        expect("snowflake", "insert into t (k, a) (select k, x from s)", &["s.k -> t.k copy", "s.x -> t.a copy"]);
    };
    update_copies_the_columns_it_sets: answered(CATALOG, "update t set b = s.x, a = s.y from s", json!(["a", "b"]),
        &["s.y -> t.a copy", "s.x -> t.b copy"]);
    update_from_decides_rows_of_the_columns_it_sets:
        expect("snowflake", "update t set a = s.x, b = s.y from s where t.k = s.k and s.flag = 1", &[
        "s.flag -> t.a rows",
        "s.k -> t.a rows",
        "s.x -> t.a copy",
        "s.flag -> t.b rows",
        "s.k -> t.b rows",
        "s.y -> t.b copy",
    ]);
    a_scalar_subquery_in_set_derives: expect("snowflake", "update t set a = (select max(x) from s where s.k = t.k)",
        &["s.k -> t.a rows", "s.x -> t.a derive"]);
    a_condition_on_the_targets_own_row_gives_no_edge:
        expect("snowflake", "update t set a = s.x from s where t.b > 0", &["s.x -> t.a copy"]);
    a_value_from_the_targets_own_row_is_an_edge: {
        expect("snowflake", "update t set a = a + 1 where b > 0", &["t.a -> t.a derive"]);
        expect("snowflake", "update t set b = a", &["t.a -> t.b copy"]);
    };
    set_of_a_column_the_target_lacks_is_refused:
        expect("snowflake", "update t set nope = s.x from s", &["refused invalid"]);
    tsql_update_through_an_alias_finds_its_target: {
        let expected = ["s.k -> t.a rows", "s.x -> t.a copy"];
        expect("tsql", "update tt set a = s.x from t as tt join s on tt.k = s.k", &expected);
        expect("snowflake", "update t as tt set a = s.x from s where tt.k = s.k", &expected);
    };
    tsql_update_from_its_target_joined:
        expect("tsql", "update t set a = s.x from t join s on t.k = s.k", &["s.k -> t.a rows", "s.x -> t.a copy"]);
    tsql_with_before_an_update_is_read:
        expect("tsql", "with c as (select k, x from s where flag = 1) update t set a = c.x from c where t.k = c.k",
        &["s.flag -> t.a rows", "s.k -> t.a rows", "s.x -> t.a copy"]);
    delete_using_decides_rows_of_every_column:
        answered(CATALOG, "delete from t using s where t.k = s.k and s.flag = 0", json!(["k", "a", "b"]), &[
        "s.flag -> t.k rows",
        "s.k -> t.k rows",
        "s.flag -> t.a rows",
        "s.k -> t.a rows",
        "s.flag -> t.b rows",
        "s.k -> t.b rows",
    ]);
    tsql_delete_from_a_join_decides_rows:
        for sql in ["delete t from t join s on t.k = s.k", "delete tt from t as tt join s on tt.k = s.k"] {
            expect("tsql", sql, &["s.k -> t.k rows", "s.k -> t.a rows", "s.k -> t.b rows"]);
        };
    merge_on_decides_rows_of_every_column:
        expect("snowflake", "merge into t using s on t.k = s.k when matched then update set a = s.x",
        &["s.k -> t.k rows", "s.k -> t.a rows", "s.x -> t.a copy", "s.k -> t.b rows"]);
    merge_update_copies_the_columns_it_sets:
        expect("snowflake", "merge into t using s on t.k = s.k when matched then update set b = s.y + 1, a = s.x",
        &["s.k -> t.k rows", "s.k -> t.a rows", "s.x -> t.a copy", "s.k -> t.b rows", "s.y -> t.b derive"]);
    merge_insert_maps_its_list:
        expect("snowflake", "merge into t using s on t.k = s.k when not matched then insert (b, k) values (s.x, s.k)",
        &["s.k -> t.k copy", "s.k -> t.a rows", "s.k -> t.b rows", "s.x -> t.b copy"]);
    // polyglot's validation refuses it, reading the values as the target's columns;
    // Snowflake maps them by position.
    merge_insert_without_a_list_is_read:
        expect("snowflake", "merge into t using s on t.k = s.k when not matched then insert values (s.k, s.x, s.y)",
        &["s.k -> t.k copy", "s.k -> t.a rows", "s.x -> t.a copy", "s.k -> t.b rows", "s.y -> t.b copy"]);
    merge_delete_decides_rows_of_every_column:
        expect("snowflake", "merge into t using s on t.k = s.k when matched and s.flag = 0 then delete", &[
        "s.flag -> t.k rows",
        "s.k -> t.k rows",
        "s.flag -> t.a rows",
        "s.k -> t.a rows",
        "s.flag -> t.b rows",
        "s.k -> t.b rows",
    ]);
    a_when_condition_decides_rows_of_its_columns_only:
        expect("snowflake", "merge into t using s on t.k = s.k when matched and s.flag = 1 then update set a = s.x",
        &["s.k -> t.k rows", "s.flag -> t.a rows", "s.k -> t.a rows", "s.x -> t.a copy", "s.k -> t.b rows"]);
    merge_of_a_column_the_target_lacks_is_refused: {
        let merge = "merge into t using s on t.k = s.k when";
        expect("snowflake", &format!("{merge} matched then update set nope = s.x"), &["refused invalid"]);
        let insert = format!("{merge} not matched then insert (k, nope) values (s.k, s.x)");
        expect("snowflake", &insert, &["refused invalid"]);
    };
    // Its columns would be the target's, which the clause does not name: refused, not read as
    // no copy, in every dialect, as polyglot-sql parses it in every one.
    merge_of_a_star_is_refused: for dialect in ["databricks", "snowflake"] {
        let merge = "merge into t using s on t.k = s.k when";
        expect(dialect, &format!("{merge} matched then update set *"), &["refused not_a_query"]);
        expect(dialect, &format!("{merge} not matched then insert *"), &["refused not_a_query"]);
    };
    // Snowflake's `INSERT ALL BY NAME` is an action of another form, refused alike.
    merge_insert_by_name_is_refused: expect("snowflake",
        "merge into t using s on t.k = s.k when not matched then insert all by name", &["refused not_a_query"]);
    merge_into_an_alias_finds_its_target:
        expect("snowflake", "merge into t as tt using s as ss on tt.k = ss.k when matched then update set tt.a = ss.x",
        &["s.k -> t.k rows", "s.k -> t.a rows", "s.x -> t.a copy", "s.k -> t.b rows"]);
    a_column_listed_twice_is_refused: {
        expect("snowflake", "insert into t (a, a) select x, y from s", &["refused invalid"]);
        expect("snowflake", "update t set a = 1, a = 2", &["refused invalid"]);
        expect("snowflake", "merge into t using s on t.k = s.k when matched then update set a = s.x, A = s.y",
            &["refused invalid"]);
        // A replace write's list names its output columns, which may not be named alike.
        expect("snowflake", "create view t (kk, KK) as select k, x from s", &["refused duplicate_column"]);
    };
}

#[test]
fn replace_statements_are_replace_writes() {
    let query = "select k, x from s";
    let mut forms: Vec<(&str, String)> = (["view", "or replace view", "secure view", "materialized view"].iter())
        .chain(&["view if not exists", "table", "or replace table", "temporary table", "transient table"])
        .map(|kind| ("snowflake", format!("create {kind} t as {query}")))
        .collect();
    forms.push(("snowflake", format!("create dynamic table t target_lag = '1 minute' warehouse = w as {query}")));
    for kind in ["create view", "create or alter view", "alter view"] {
        forms.push(("tsql", format!("{kind} t as {query}")));
    }
    forms.push(("tsql", "select k, x into t from s".to_string()));
    for kind in ["view", "or replace view", "table", "or replace table"] {
        forms.push(("duckdb", format!("create {kind} t as {query}")));
    }
    let copied = ["s.k -> t.k copy", "s.x -> t.x copy"];
    for (dialect, sql) in &forms {
        // The target's columns become the query's: `x`, not the catalog's `a` and `b`.
        let found = traced(dialect, &[("t", sql), ("v", "select x from t")]);
        assert_eq!(found, [copied.to_vec(), vec!["t.x -> v.x copy"]], "{dialect}: {sql}");
    }
    // polyglot keeps the `INTO` of a set operation on its left branch.
    let union = "select k, x into t from s union all select k, z from u";
    assert_eq!(trace("tsql", union), ["s.k -> t.k copy", "u.k -> t.k copy", "s.x -> t.x copy", "u.z -> t.x copy"]);
    let mut temporary = writes("tsql", &[("#t", "select k, x into #t from s")]);
    temporary["catalog"] = json!([{"relation": "s", "columns": ["k", "x"]}]);
    assert_eq!(outline(&respond(&temporary)[0]), ["s.k -> #t.k copy", "s.x -> #t.x copy"]);
}

#[test]
fn a_view_answers_as_its_bare_query() {
    let query = "with c as (select k, x from s where flag = 1) \
        select c.k, c.x + u.z as total, (select max(y) from s) as top from c join u on c.k = u.k";
    let bare = &respond(&writes("snowflake", &[("t", query)]))[0];
    let view = &respond(&writes("snowflake", &[("t", &format!("create view t as {query}"))]))[0];
    assert!(!outline(bare).is_empty());
    assert_eq!(view, bare);
}

#[test]
fn a_statement_for_another_target_is_refused() {
    assert_eq!(trace("snowflake", "create view v as select k, x from s"), ["refused other_target"]);
    // Traced under its request's target before plan 006, a name that its answer got wrong.
    assert_eq!(trace("tsql", "select k, x into v from s"), ["refused other_target"]);
    // A statement's target is completed by the defaults, as the request's is.
    let mut request = writes("snowflake", &[("t", "create view db.sch.t as select k from s")]);
    request["defaults"] = json!({"database": "db", "schema": "sch"});
    request["catalog"] = json!([{"relation": "s", "columns": ["k"]}]);
    assert_eq!(outline(&respond(&request)[0]), ["s.k -> t.k copy"]);
}

#[test]
fn a_contribute_write_to_an_unknown_target_is_a_gap() {
    let gap = "gap unknown_relation w -";
    let answer = &respond(&writes("snowflake", &[("w", "insert into w (K, \"Aa\") select k, x from s")]))[0];
    assert_eq!(answer["columns"], Value::Null);
    // Spelled as the statement spells them, for the catalog does not.
    assert_eq!(outline(answer), ["s.k -> w.K copy", "s.x -> w.\"Aa\" copy", gap]);
    let statements: [(&str, &[&str]); 4] = [
        ("insert into w select k, x from s", &[gap]),
        ("update w set a = s.x from s where w.k = s.k", &["s.k -> w.a rows", "s.x -> w.a copy", gap]),
        ("delete from w using s where w.k = s.k", &[gap]),
        (
            "merge into w using s on w.k = s.k when matched then update set a = s.x \
                when not matched then insert values (s.k, s.x)",
            &["s.x -> w.a copy", gap],
        ),
    ];
    for (sql, expected) in statements {
        assert_eq!(traced("snowflake", &[("w", sql)]).remove(0), expected, "{sql}");
    }
}

#[test]
fn set_with_a_qualified_target_is_read() {
    let expected = ["s.k -> t.a rows", "s.x -> t.a copy"];
    for dialect in ["snowflake", "tsql", "duckdb"] {
        assert_eq!(trace(dialect, "update t set t.a = s.x from s where t.k = s.k"), expected, "{dialect}");
    }
    assert_eq!(trace("tsql", "update x set x.a = s.x from t as x join s on x.k = s.k"), expected);
    assert_eq!(trace("snowflake", "update t set t.b = s.y, a = s.x from s"), ["s.x -> t.a copy", "s.y -> t.b copy"]);
    // A target that names no column, such as a tuple, is not read.
    assert_eq!(trace("snowflake", "update t set (a, b) = (select x, y from s)"), ["refused not_a_query"]);
}

#[test]
fn tsql_a_quote_in_a_national_string_is_refused_once_generated() {
    // polyglot-sql 0.14.3 generates `N'it''s'` as `N'it's'`, which does not parse again: a
    // statement, generated into its piece, is refused; the same query, not generated, is not.
    // A defect of the code, in `docs/differences.md`.
    let condition = "where flag <> N'it''s'";
    assert_eq!(trace("tsql", &format!("create view t as select k from s {condition}")), ["refused unparsed"]);
    assert_eq!(trace("tsql", &format!("select k from s {condition}")), ["s.flag -> t.k rows", "s.k -> t.k copy"]);
}
