// SPDX-License-Identifier: EUPL-1.2
//! The command `graph`, run as a caller runs it on the scripts of `tests/scripts/`, as
//! `docs/protocol.md` gives it. Expectations from plan 008, the probe's observations of
//! the scripts of SQL Server's tools, and the rulings.

use std::process::Output;

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
