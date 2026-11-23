// SPDX-License-Identifier: EUPL-1.2
//! The command `graph`, run as a caller runs it on the scripts of `tests/scripts/`, as
//! `docs/protocol.md` gives it. Expectations from plan 008, the probe's observations of
//! the scripts of SQL Server's tools, and the rulings.

use std::process::Output;

use serde_json::{Value, json};

use super::{assert_refused, binary, run};

/// The binary's output for `graph` and `arguments`, run from the repository's root.
fn graph(arguments: &[&str]) -> Output {
    let command = binary().current_dir(env!("CARGO_MANIFEST_DIR")).arg("graph").args(arguments).output();
    command.expect("the binary runs")
}

/// The lines that `graph` prints for `arguments`, which it does not refuse.
fn printed(arguments: &[&str]) -> Vec<String> {
    let output = graph(arguments);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
    String::from_utf8(output.stdout).expect("the output is UTF-8").lines().map(str::to_string).collect()
}

/// The lines that `graph` prints for the scripts at `paths` in T-SQL.
fn tsql(paths: &[&str]) -> Vec<String> {
    printed(&[&["--dialect", "tsql"], paths].concat())
}

/// The ids of the writes among `lines`, the first word of each line that is no finding.
fn ids(lines: &[String]) -> Vec<&str> {
    let writes = lines.iter().filter(|line| !line.split(' ').next().is_some_and(|word| word.ends_with(':')));
    writes.filter_map(|line| line.split(' ').next()).collect()
}

#[test]
fn a_command_without_a_dialect_is_refused() {
    assert_refused(&graph(&["tests/scripts/tsql"]), "--dialect");
    assert_refused(&graph(&["--dialect", "bigquery", "tests/scripts/tsql"]), "bigquery");
    assert!(!tsql(&["tests/scripts/tsql"]).is_empty());
}

#[test]
fn an_unknown_command_is_refused() {
    assert_refused(&run(&["migrate", "--dialect", "tsql", "tests/scripts/tsql"]), "migrate");
}

#[test]
fn an_unknown_option_is_refused() {
    assert_refused(&graph(&["--dialect", "tsql", "--verbose", "tests/scripts/tsql"]), "--verbose");
    assert!(!tsql(&["tests/scripts/tsql"]).is_empty());
}

#[test]
fn a_path_that_does_not_exist_is_refused() {
    assert_refused(&graph(&["--dialect", "tsql", "tests/scripts/tsql/none.sql"]), "tests/scripts/tsql/none.sql");
    assert!(!tsql(&["tests/scripts/tsql/02-views.sql"]).is_empty());
}

#[test]
fn statements_split_at_semicolons() {
    // A `;` in a string, a comment or a `$$` body splits nothing; `;;` holds no statement.
    let snowflake = printed(&["--dialect", "snowflake", "tests/scripts/snowflake/01-ported.sql"]);
    let path = "tests/scripts/snowflake/01-ported.sql";
    let writes = [format!("{path}:2:1 V_ORDERS <- RAW.ORDERS"), format!("{path}:11:1 FACT <- V_ORDERS")];
    // The procedure, and a table that polyglot refuses, are skipped.
    let rest = [format!("{path}:13:1 FACT <- FACT"), "skipped: 2".to_string()];
    assert_eq!(snowflake, [writes, rest].concat());
    let duckdb = printed(&["--dialect", "duckdb", "tests/scripts/duckdb/01-fixture.sql"]);
    let path = "tests/scripts/duckdb/01-fixture.sql";
    let writes = [format!("{path}:3:1 stg_orders <- raw_orders"), format!("{path}:4:1 orders <- stg_orders")];
    assert_eq!(duckdb, [writes.as_slice(), &["skipped: 2".to_string()]].concat());
}

#[test]
fn tsql_batches_end_at_go() {
    // `go`, `GO 2` and `GO` end batches; `GO` in a comment or a string, and `[GO]`, do not.
    let path = "tests/scripts/tsql/05-load.sql";
    let expected = [
        format!("{path}:5:1 dbo.Fact <- dbo.Stage"),
        format!("{path}:7:1 dbo.Fact <- dbo.Fact"),
        format!("{path}:8:3 dbo.Fact <- dbo.Fact"),
        format!("{path}:11:1 dbo.Gone <- dbo.Fact"),
        format!("{path}:15:1 dbo.Dim <- dbo.Dim, dbo.Stage"),
        "skipped: 1".to_string(),
    ];
    assert_eq!(tsql(&[path]), expected);
}

#[test]
fn tsql_sqlcmd_lines_end_a_batch() {
    let path = "tests/scripts/tsql/06-sqlpackage.sql";
    let expected = [
        format!("{path}:7:1 [dbo].[vA] <- [dbo].[Orders]"),
        format!("{path}:11:1 [dbo].[vB] <- [dbo].[vA]"),
        "skipped: 1".to_string(),
    ];
    assert_eq!(tsql(&[path]), expected);
}

#[test]
fn tsql_statements_without_semicolons_are_split() {
    // The `INSERT` grows over the line of its `SELECT`, which starts a line.
    let path = "tests/scripts/tsql/05-load.sql";
    let expected = [
        format!("{path}:5:1 dbo.Fact <- dbo.Stage"),
        format!("{path}:7:1 dbo.Fact <- dbo.Fact"),
        format!("{path}:8:3 dbo.Fact <- dbo.Fact"),
    ];
    assert_eq!(tsql(&[path])[..3], expected);
}

#[test]
fn tsql_transaction_words_end_a_statement() {
    // `COMMIT` and `WAITFOR` start statements: the `INSERT`s before them end there.
    let path = "tests/scripts/tsql/15-transaction.sql";
    let expected = [
        format!("{path}:2:1 dbo.w <- dbo.u"),
        format!("{path}:5:1 dbo.x <- dbo.w"),
        format!("{path}:7:1 dbo.q <- dbo.x"),
        "skipped: 3".to_string(),
    ];
    assert_eq!(tsql(&[path]), expected);
}

#[test]
fn tsql_a_view_is_read_whole() {
    // So is a procedure, whose `INSERT` is no write of the script.
    let path = "tests/scripts/tsql/07-routines.sql";
    let expected = [
        format!("{path}:1:1 [dbo].[vSign] <- [dbo].[Orders]"),
        format!("{path}:9:1 [dbo].[vLate] <- [dbo].[Orders], [dbo].[Returns]"),
        "skipped: 1".to_string(),
    ];
    assert_eq!(tsql(&[path]), expected);
}

#[test]
fn tsql_preamble_is_skipped() {
    // `USE`, two `SET`s, the existence test and its `DROP VIEW`: four statements skipped, and
    // `USE`, which sets the defaults.
    let path = "tests/scripts/tsql/02-views.sql";
    assert_eq!(tsql(&[path]), [format!("{path}:12:1 [dbo].[vCustomer] <- [dbo].[Customer]"), "skipped: 4".to_string()]);
}

#[test]
fn tsql_existence_check_is_read() {
    // The write is named by its `EXEC`'s place.
    let path = "tests/scripts/tsql/03-existence-check.sql";
    assert_eq!(tsql(&[path]), [format!("{path}:9:1 [dbo].[vOpen] <- [dbo].[Orders]"), "skipped: 3".to_string()]);
}

#[test]
fn other_dynamic_sql_is_not_read() {
    // `EXEC` of a string or a variable, `sp_executesql` of a variable or of a string that
    // does not parse; `DECLARE` and a procedure's `EXEC` skipped.
    let path = "tests/scripts/tsql/04-dynamic.sql";
    let not_read = |place: &str| format!("not_read: {path}:{place} dynamic SQL");
    let expected = [not_read("1:1"), not_read("4:1"), not_read("5:1"), not_read("6:1"), "skipped: 2".to_string()];
    assert_eq!(tsql(&[path]), expected);
}

#[test]
fn a_refused_write_is_reported_with_its_place() {
    // SSMS's `INSERT` without `INTO`, refused, is reported; its `SELECT`, read alone, and a
    // refused `IF`, which is no write, are counted.
    let path = "tests/scripts/tsql/08-refused.sql";
    let found = tsql(&[path]);
    let writes = [format!("{path}:1:1 dbo.StageA <- dbo.StageA"), format!("{path}:4:1 dbo.StageA <- dbo.StageA")];
    assert_eq!(found[..2], writes);
    assert!(found[2].starts_with(&format!("refused: {path}:2:1 ")), "{found:?}");
    assert!(found[2].contains("Expected Into"), "{found:?}");
    assert_eq!(found[3..], ["skipped: 2"]);
}

#[test]
fn a_bom_and_utf16_are_decoded() {
    // A UTF-8 BOM, UTF-16 LE and UTF-16 BE, each with CRLF: one text, one place for each
    // token, in characters, the BOM not counted.
    for name in ["11-utf8-bom", "12-utf16le", "13-utf16be"] {
        let path = format!("tests/scripts/tsql/{name}.sql");
        let expected = [
            format!("{path}:1:1 [dbo].[vCafé] <- [dbo].[Thé]"),
            format!("{path}:5:11 [dbo].[Tarte] <- [dbo].[vCafé]"),
            "skipped: 0".to_string(),
        ];
        assert_eq!(tsql(&[&path]), expected);
    }
}

#[test]
fn a_file_that_does_not_decode_is_reported() {
    // Windows-1252, with no BOM, is not UTF-8: reported by its path, and the others read.
    let (cp1252, views) = ("tests/scripts/tsql/14-cp1252.sql", "tests/scripts/tsql/02-views.sql");
    let expected = [
        format!("{views}:12:1 [dbo].[vCustomer] <- [dbo].[Customer]"),
        format!("not_read: {cp1252} not UTF-8, nor UTF-16 after a byte order mark"),
        "skipped: 4".to_string(),
    ];
    assert_eq!(tsql(&[cp1252, views]), expected);
}

#[test]
fn write_ids_are_their_places() {
    // The path as given, then the line and the column of the write's first token, from 1:
    // past an indent, and in characters, past an accented letter, before CRLF.
    let path = "./tests/scripts/tsql/05-load.sql";
    let places = ["5:1", "7:1", "8:3", "11:1", "15:1"].map(|place| format!("{path}:{place}"));
    assert_eq!(ids(&tsql(&[path])), places);
    let path = "tests/scripts/tsql/11-utf8-bom.sql";
    assert_eq!(ids(&tsql(&[path])), [format!("{path}:1:1"), format!("{path}:5:11")]);
}

#[test]
fn a_directory_is_read_in_path_order() {
    // Its files ending `.sql`, at any depth, sorted by path; `notes.txt` is not read.
    let expected = [
        "tests/scripts/tsql/order/a/c.sql:1:1 dbo.vC <- dbo.Orders",
        "tests/scripts/tsql/order/a.sql:1:1 dbo.vA <- dbo.Orders",
        "tests/scripts/tsql/order/b.sql:1:1 dbo.vB <- dbo.Orders",
        "skipped: 0",
    ];
    assert_eq!(tsql(&["tests/scripts/tsql/order"]), expected);
}

/// The ids of the writes that `graph` prints for `paths` in `dialect`, with `options`.
fn ordered(dialect: &str, options: &[&str], paths: &[&str]) -> Vec<String> {
    let lines = printed(&[&["--dialect", dialect], options, paths].concat());
    ids(&lines).into_iter().map(str::to_string).collect()
}

/// Where `id` comes among `ids`.
fn place(ids: &[String], id: &str) -> usize {
    ids.iter().position(|other| other == id).unwrap_or_else(|| panic!("{id} not among {ids:?}"))
}

#[test]
fn a_database_option_without_a_schema_takes_the_usual_one() {
    // `vTop` reads the database's view `vBase`, named without a schema: it comes after it only
    // when the usual schema completes that name, `dbo`, `PUBLIC` or `main`.
    let scripts = [
        ("tsql", "tests/scripts/tsql/10-defaults.sql", "Sales", 3),
        ("snowflake", "tests/scripts/snowflake/03-defaults.sql", "SALES", 2),
        ("duckdb", "tests/scripts/duckdb/02-defaults.sql", "sales", 2),
    ];
    for (dialect, path, database, line) in scripts {
        let (top, base) = (format!("{path}:1:1"), format!("{path}:{line}:1"));
        assert_eq!(ordered(dialect, &["--database", database], &[path]), [base.clone(), top.clone()], "{dialect}");
        assert_eq!(ordered(dialect, &[], &[path]), [top, base], "{dialect}");
    }
}

#[test]
fn use_changes_the_defaults() {
    // After `USE [Sales]`, `dbo.vBase` is the view that `vTop` reads, written before it.
    let path = "tests/scripts/tsql/09-use.sql";
    let found = ordered("tsql", &[], &[path]);
    assert!(place(&found, &format!("{path}:9:1")) < place(&found, &format!("{path}:3:1")), "{found:?}");
}

#[test]
fn use_without_a_schema_takes_the_usual_one() {
    // After `USE [Sales]`, `vPlain` is `Sales.dbo.vPlain`, which `vBare` reads.
    let path = "tests/scripts/tsql/09-use.sql";
    let found = ordered("tsql", &[], &[path]);
    assert!(place(&found, &format!("{path}:11:1")) < place(&found, &format!("{path}:5:1")), "{found:?}");
}

#[test]
fn snowflake_use_of_a_qualified_schema() {
    // `USE SCHEMA SALES.MART` sets both defaults; `USE SCHEMA STAGE` keeps the database.
    let path = "tests/scripts/snowflake/02-use.sql";
    let expected = [
        format!("{path}:3:1 SALES.MART.V_BASE <- SALES.RAW.ORDERS"),
        format!("{path}:2:1 V_TOP <- V_BASE"),
        format!("{path}:6:1 V_SRC <- SALES.RAW.ORDERS"),
        format!("{path}:5:1 V_LOW <- SALES.STAGE.V_SRC"),
        "skipped: 0".to_string(),
    ];
    assert_eq!(printed(&["--dialect", "snowflake", path]), expected);
}

/// The path of the migration script `name`.
fn migration(name: &str) -> String {
    format!("tests/scripts/tsql/migration/{name}.sql")
}

#[test]
fn graph_orders_writes_after_what_they_read() {
    // Given in the order b, a, c, d: the writes of `dbo.Fact`, the cycle, the two writes of
    // `vA`, then `vB`, which reads `vA`, then `vC`, which reads `dbo.Fact` and `vB`.
    let [a, b, c, d] = ["a-load", "b-views", "c-base", "d-redefine"].map(migration);
    let expected = [
        format!("{a}:1:1 dbo.Fact <- dbo.Stage"),
        format!("{a}:2:1 dbo.Fact <- dbo.Stage2"),
        format!("{a}:3:1 dbo.Fact <- dbo.Fact"),
        format!("{a}:5:1 dbo.Loop1 <- dbo.Loop2"),
        format!("{a}:6:1 dbo.Loop2 <- dbo.Loop1"),
        format!("{c}:3:1 [dbo].[vA] <- dbo.Stage"),
        format!("{d}:1:1 dbo.vA <- dbo.Stage2"),
        format!("{b}:1:1 dbo.vB <- dbo.vA"),
        format!("{b}:3:1 [dbo].[vC] <- dbo.Fact, dbo.vB"),
        format!("cycle: {a}:5:1, {a}:6:1 (dbo.Loop1, dbo.Loop2)"),
        format!("replaced_twice: [dbo].[vA] ({c}:3:1, {d}:1:1)"),
        "skipped: 1".to_string(),
    ];
    assert_eq!(tsql(&[&b, &a, &c, &d]), expected);
}

#[test]
fn graph_orders_a_contribute_write_to_an_undeclared_target() {
    // No catalog knows `dbo.Fact`: the `DELETE` still reads `vGone` in its condition.
    let path = migration("g-undeclared");
    let expected = [
        format!("{path}:3:1 dbo.vGone <- dbo.Stage"),
        format!("{path}:1:1 dbo.Fact <- dbo.Fact, dbo.vGone"),
        "skipped: 0".to_string(),
    ];
    assert_eq!(tsql(&[&path]), expected);
}

#[test]
fn graph_keeps_reading_order_where_free() {
    // `vY` and `vZ` are free first: `vY`, the earlier, comes first, then `vZ`, then `vX`.
    let path = migration("e-free");
    let expected = [
        format!("{path}:3:1 dbo.vY <- dbo.Orders"),
        format!("{path}:5:1 dbo.vZ <- dbo.Orders"),
        format!("{path}:1:1 dbo.vX <- dbo.vZ"),
        "skipped: 0".to_string(),
    ];
    assert_eq!(tsql(&[&path]), expected);
}

#[test]
fn graph_reports_a_cycle() {
    // Two inserts that read each other's target, in reading order, at the earlier's turn.
    let path = migration("a-load");
    let found = tsql(&[&path]);
    let ids = [1, 2, 3, 5, 6].map(|line| format!("{path}:{line}:1"));
    assert_eq!(self::ids(&found), ids);
    assert_eq!(found[5..], [format!("cycle: {path}:5:1, {path}:6:1 (dbo.Loop1, dbo.Loop2)"), "skipped: 0".to_string()]);
}

#[test]
fn graph_puts_a_write_of_its_own_target_after_earlier_ones() {
    // The `UPDATE` reads its own target and waits only on the `SELECT ... INTO` before it;
    // the `INSERT` waits on both; the view that reads `dbo.Fact`, on all three.
    let path = migration("f-own");
    let expected = [
        format!("{path}:3:1 dbo.Fact <- dbo.Stage"),
        format!("{path}:4:1 dbo.Fact <- dbo.Fact"),
        format!("{path}:5:1 dbo.Fact <- dbo.Stage2"),
        format!("{path}:1:1 dbo.vFact <- dbo.Fact"),
        "skipped: 0".to_string(),
    ];
    assert_eq!(tsql(&[&path]), expected);
}

#[test]
fn graph_reports_a_target_replaced_twice() {
    // `vA` is created, then created or altered: reported, and `vB` comes after both.
    let [b, c, d] = ["b-views", "c-base", "d-redefine"].map(migration);
    let expected = [
        format!("{c}:3:1 [dbo].[vA] <- dbo.Stage"),
        format!("{d}:1:1 dbo.vA <- dbo.Stage2"),
        format!("{b}:1:1 dbo.vB <- dbo.vA"),
        format!("{b}:3:1 [dbo].[vC] <- dbo.Fact, dbo.vB"),
        format!("replaced_twice: [dbo].[vA] ({c}:3:1, {d}:1:1)"),
        "skipped: 1".to_string(),
    ];
    assert_eq!(tsql(&[&b, &c, &d]), expected);
}

#[test]
fn graph_counts_skipped_statements() {
    // SSMS's table script: two `SET`s, `CREATE TABLE`, `ALTER TABLE` and an extended
    // property's `EXEC`; its `USE` sets the defaults. Counted across files.
    let (tables, views) = ("tests/scripts/tsql/01-tables.sql", "tests/scripts/tsql/02-views.sql");
    assert_eq!(tsql(&[tables]), ["skipped: 5"]);
    let both = [format!("{views}:12:1 [dbo].[vCustomer] <- [dbo].[Customer]"), "skipped: 9".to_string()];
    assert_eq!(tsql(&[tables, views]), both);
}

#[test]
fn graph_json_lists_writes_and_findings() {
    let [a, c, d] = ["a-load", "c-base", "d-redefine"].map(migration);
    let (refused, cp1252) = ("tests/scripts/tsql/08-refused.sql", "tests/scripts/tsql/14-cp1252.sql");
    let output = graph(&["--dialect", "tsql", "--json", &a, &c, &d, refused, cp1252]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let mut graph: Value = serde_json::from_slice(&output.stdout).expect("the output is JSON");
    // polyglot's message, text for a person.
    let message = graph["findings"][0]["detail"].take();
    assert!(message.as_str().is_some_and(|message| message.contains("Expected Into")), "{message}");
    let write = |id: String, target: &str, read: &str| json!({"id": id, "target": target, "reads": [read]});
    let writes = [
        write(format!("{a}:1:1"), "dbo.Fact", "dbo.Stage"),
        write(format!("{a}:2:1"), "dbo.Fact", "dbo.Stage2"),
        write(format!("{a}:3:1"), "dbo.Fact", "dbo.Fact"),
        write(format!("{a}:5:1"), "dbo.Loop1", "dbo.Loop2"),
        write(format!("{a}:6:1"), "dbo.Loop2", "dbo.Loop1"),
        write(format!("{c}:3:1"), "[dbo].[vA]", "dbo.Stage"),
        write(format!("{d}:1:1"), "dbo.vA", "dbo.Stage2"),
        write(format!("{refused}:1:1"), "dbo.StageA", "dbo.StageA"),
        write(format!("{refused}:4:1"), "dbo.StageA", "dbo.StageA"),
    ];
    let findings = [
        json!({"kind": "refused", "place": format!("{refused}:2:1"), "detail": null}),
        json!({"kind": "not_read", "place": cp1252, "detail": "not UTF-8, nor UTF-16 after a byte order mark"}),
        json!({"kind": "cycle", "writes": [format!("{a}:5:1"), format!("{a}:6:1")], "relations": ["dbo.Loop1", "dbo.Loop2"]}),
        json!({"kind": "replaced_twice", "relation": "[dbo].[vA]", "writes": [format!("{c}:3:1"), format!("{d}:1:1")]}),
    ];
    assert_eq!(graph, json!({"writes": writes, "findings": findings, "skipped": 3}));
}
