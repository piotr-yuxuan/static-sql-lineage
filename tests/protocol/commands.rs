// SPDX-License-Identifier: EUPL-1.2
//! The commands `graph`, `trace`, `uses` and `outline`, run as a caller runs them on the scripts of
//! `tests/scripts/`, as `docs/protocol.md` gives them. Expectations from plans 008 and 009,
//! the probe's observations of the scripts of SQL Server's tools, the rulings and the Edge
//! entry of `docs/concepts.md`; and the README's examples.

use std::process::Output;

use serde_json::{Value, json};

use super::{assert_refused, assert_usage_error, command, lacks, lines, printed_json, read, stdout};

/// Asserts that the command of the transcript `name` of `tests/scripts/expected/`, its
/// arguments split at spaces, prints from the repository's root what follows it there, byte
/// for byte.
#[track_caller]
fn prints(name: &str) {
    let file = read(&format!("tests/scripts/expected/{name}.txt"));
    let (command, expected) = transcript(&file).expect("the file is a transcript");
    assert_eq!(stdout(&command.split_whitespace().collect::<Vec<_>>()), expected, "{command}");
}

/// The command of the transcript `text`, the arguments after `$ static-sql-lineage ` on its
/// first line, and what follows that line; none for a text whose first line is no command.
fn transcript(text: &str) -> Option<(&str, &str)> {
    let (first, printed) = text.split_once('\n')?;
    Some((first.strip_prefix("$ static-sql-lineage ")?, printed))
}

/// The binary's output for `graph` and `arguments`, run from the repository's root.
fn graph(arguments: &[&str]) -> Output {
    command(&[&["graph"], arguments].concat())
}

/// The lines that `graph` prints for `arguments`, which it does not refuse.
fn printed(arguments: &[&str]) -> Vec<String> {
    lines(&[&["graph"], arguments].concat())
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

cases! {
    a_command_without_a_dialect_is_refused: {
        assert_usage_error(&graph(&["tests/scripts/tsql"]), "--dialect");
        let unbuilt = graph(&["--dialect", "bigquery", "tests/scripts/tsql"]);
        assert_usage_error(&unbuilt, "bigquery");
        assert_usage_error(&unbuilt, "snowflake, tsql, duckdb");
        assert!(!tsql(&["tests/scripts/tsql"]).is_empty());
    };
    an_unknown_command_is_refused:
        assert_usage_error(&command(&["migrate", "--dialect", "tsql", "tests/scripts/tsql"]), "migrate");
    an_unknown_option_is_refused: {
        assert_usage_error(&graph(&["--dialect", "tsql", "--verbose", "tests/scripts/tsql"]), "--verbose");
        assert!(!tsql(&["tests/scripts/tsql"]).is_empty());
    };
    double_dash_ends_the_options: assert_eq!(
        stdout(&["graph", "--dialect", "duckdb", "--", "examples/tpch"]),
        stdout(&["graph", "--dialect", "duckdb", "examples/tpch"])
    );
    a_path_that_does_not_exist_is_refused: {
        let path = "tests/scripts/tsql/none.sql";
        assert_refused(&graph(&["--dialect", "tsql", path]), path);
        assert!(!tsql(&["tests/scripts/tsql/02-views.sql"]).is_empty());
    };
    // `go`, `GO 2` and `GO` end batches; `GO` in a comment or a string, and `[GO]`, do not.
    tsql_batches_end_at_go: prints("graph-05-load");
    tsql_sqlcmd_lines_end_a_batch: prints("graph-06-sqlpackage");
    // `COMMIT` and `WAITFOR` start statements: the `INSERT`s before them end there.
    tsql_transaction_words_end_a_statement: prints("graph-15-transaction");
    // polyglot stops after the `IF`'s condition, at `INSERT` or `ROLLBACK`: each a statement of
    // its own, the conditions and the `ROLLBACK` skipped. The `INSERT` of `dbo.t` reads `dbo.d`.
    tsql_a_statement_after_an_if_on_its_line_is_read: prints("graph-16-if-on-a-line");
    // So is a procedure, whose `INSERT` is no write of the script.
    tsql_a_view_is_read_whole: prints("graph-07-routines");
    // `USE`, two `SET`s, the existence test and its `DROP VIEW`: four statements skipped, and
    // `USE`, which sets the defaults.
    tsql_preamble_is_skipped: prints("graph-02-views");
    // The write is named by its `EXEC`'s place.
    tsql_existence_check_is_read: prints("graph-03-existence-check");
    // `EXEC` of a string or a variable, `sp_executesql` of a variable or of a string that
    // does not parse; `DECLARE` and a procedure's `EXEC` skipped.
    other_dynamic_sql_is_not_read: prints("graph-04-dynamic");
    // A UTF-8 BOM, UTF-16 LE and UTF-16 BE, each with CRLF: one text, one place for each
    // token, in characters, the BOM not counted.
    a_bom_and_utf16_are_decoded: {
        prints("graph-11-utf8-bom");
        prints("graph-12-utf16le");
        prints("graph-13-utf16be");
    };
    // Windows-1252, with no BOM, is not UTF-8: reported by its path, and the others read.
    a_file_that_does_not_decode_is_reported: prints("graph-14-cp1252-02-views");
    // Its files ending `.sql`, in any case, at any depth, sorted by path; `notes.txt` is not
    // read.
    a_directory_is_read_in_path_order: prints("graph-order");
    // `USE SCHEMA SALES.MART` sets both defaults; `USE SCHEMA STAGE` keeps the database.
    snowflake_use_of_a_qualified_schema: prints("graph-snowflake-02-use");
    // A routine's dollar-quoted body holds `;` and splits nothing; it, `DO` and `SET` are skipped.
    postgresql_dollar_quotes_split_nothing: prints("graph-postgresql-01-dollar");
    redshift_dollar_quotes_split_nothing: prints("graph-redshift-01-dollar");
    databricks_dollar_quotes_split_nothing: prints("graph-databricks-01-dollar");
    // `USE CATALOG sales` sets the schema `default`; `USE mart` and `USE DATABASE stage` set
    // the schema; `USE SCHEMA crm.core` sets both.
    databricks_use_sets_the_schema: prints("graph-databricks-03-use");
    // A `MERGE` of `*`, which answers refuse, still reads its sources and writes its target.
    graph_orders_a_merge_of_a_star: prints("graph-databricks-04-merge");
    // Given in the order b, a, c, d: the writes of `dbo.Fact`, the cycle, the two writes of
    // `vA`, then `vB`, which reads `vA`, then `vC`, which reads `dbo.Fact` and `vB`.
    graph_orders_writes_after_what_they_read: prints("graph-migration-b-a-c-d");
    // No catalog knows `dbo.Fact`: the `DELETE` still reads `vGone` in its condition.
    graph_orders_a_contribute_write_to_an_undeclared_target: prints("graph-migration-g-undeclared");
    // `vY` and `vZ` are free first: `vY`, the earlier, comes first, then `vZ`, then `vX`.
    graph_keeps_reading_order_where_free: prints("graph-migration-e-free");
    // The `UPDATE` reads its own target and waits only on the `SELECT ... INTO` before it;
    // the `INSERT` waits on both; the view that reads `dbo.Fact`, on all three.
    graph_puts_a_write_of_its_own_target_after_earlier_ones: prints("graph-migration-f-own");
    // `vA` is created, then created or altered: reported, and `vB` comes after both.
    graph_reports_a_target_replaced_twice: prints("graph-migration-b-c-d");
    // From the view to the table that the loads fill, then to the tables that they read, which
    // no write fills; the refused load of the table is met on the way.
    trace_follows_edges_to_base_relations: prints("trace-lineage-vBase-Total");
    // Each load reads the other's target: the walk comes back to its column, and stops.
    trace_stops_at_a_cycle: prints("trace-19-cycle");
    // `vShared` is reached through `vLeft` and through `vRight`; what it reads is listed once,
    // and no cycle.
    trace_lists_a_shared_column_once: prints("trace-20-diamond");
    // From a table that a load reads to the table that it fills, then to the views over it;
    // the refused load reads the table too.
    uses_follows_edges_downstream: prints("uses-lineage-Stage-Amount");
    // No write fills `dbo.Stage2`: its column is no answer's output and no edge's child, only
    // the parent of its readers' edges.
    uses_of_a_base_column_known_only_by_its_readers: prints("uses-lineage-Stage2-Amount");
    // SSMS's table script: two `SET`s, `ALTER TABLE` and an extended property's `EXEC`; its
    // `USE` sets the defaults, and its `CREATE TABLE` declares. Counted across files.
    graph_counts_skipped_statements: {
        prints("graph-01-tables");
        prints("graph-01-tables-02-views");
    };
    // The view's `*` reads the declared columns, the column `sparse` and `[Blob]` among them.
    declarations_feed_the_catalog: {
        prints("trace-18-storage-vDoc-sparse");
        prints("trace-18-storage-vDoc-Blob");
    };
    // No declaration, edge or output names these; nor does an argument with no relation.
    an_unknown_column_is_refused: {
        assert_refused(&command(&["trace", "--dialect", "tsql", "dbo.vBase.Nope", LINEAGE]), "dbo.vBase.Nope");
        assert_refused(&command(&["uses", "--dialect", "tsql", "dbo.Missing.Id", LINEAGE]), "dbo.Missing.Id");
        assert_refused(&command(&["trace", "--dialect", "tsql", "Total", LINEAGE]), "Total");
        // A known column with no edge downstream gives an empty walk.
        assert_eq!(walked("uses", "dbo.vTop.Total", &[LINEAGE]), [""]);
    };
    an_unknown_write_is_refused: {
        let id = format!("{LINEAGE}/2-load.sql:9:1");
        assert_refused(&command(&["outline", "--dialect", "tsql", &id, LINEAGE]), &id);
        assert_usage_error(&command(&["outline", "--dialect", "tsql", LINEAGE]), "outline");
    };
}

#[test]
fn statements_split_at_semicolons() {
    // A `;` in a string, a comment or a `$$` body splits nothing; `;;` holds no statement.
    let snowflake = printed(&["--dialect", "snowflake", "tests/scripts/snowflake/01-ported.sql"]);
    let path = "tests/scripts/snowflake/01-ported.sql";
    let writes = [format!("{path}:2:1 V_ORDERS <- RAW.ORDERS"), format!("{path}:11:1 FACT <- V_ORDERS")];
    // The procedure is skipped; a table that polyglot refuses is a refused declaration.
    assert_eq!(snowflake[..3], [writes.as_slice(), &[format!("{path}:13:1 FACT <- FACT")]].concat());
    assert!(snowflake[3].starts_with(&format!("refused: {path}:12:1 ")), "{snowflake:?}");
    assert_eq!(snowflake[4..], ["skipped: 1"]);
    // The declaration of `raw_orders` is not skipped; its `INSERT ... VALUES` is.
    prints("graph-duckdb-01-fixture");
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
fn a_refused_write_is_reported_with_its_place() {
    // SSMS's `INSERT` without `INTO`, refused, is reported; its `SELECT`, read alone, the `IF`
    // and the `DROP VIEW` on one line, and a refused `ALTER TABLE`, which is no write, are
    // counted.
    let path = "tests/scripts/tsql/08-refused.sql";
    let found = tsql(&[path]);
    let writes = [format!("{path}:1:1 dbo.StageA <- dbo.StageA"), format!("{path}:4:1 dbo.StageA <- dbo.StageA")];
    assert_eq!(found[..2], writes);
    assert!(found[2].starts_with(&format!("refused: {path}:2:1 ")), "{found:?}");
    assert!(found[2].contains("Expected Into"), "{found:?}");
    // At the place in the file of `dbo`, where polyglot stops, not its place in the statement.
    assert!(found[2].ends_with(", at 2:8") && !found[2].contains("line 1"), "{found:?}");
    assert_eq!(found[3..], ["skipped: 4"]);
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

#[cfg(unix)]
#[test]
fn a_link_to_a_directory_is_not_followed() {
    // A link to its own directory, found under it, would read the directory again and again.
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("links-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    std::fs::write(scratch.join("a.sql"), "CREATE VIEW dbo.vA AS SELECT Id FROM dbo.Orders\n").expect("a script");
    std::os::unix::fs::symlink(".", scratch.join("again")).expect("a link");
    let path = scratch.display().to_string();
    let found = tsql(&[&path]);
    std::fs::remove_dir_all(&scratch).expect("the scratch directory removed");
    assert_eq!(found, [format!("{path}/a.sql:1:1 dbo.vA <- dbo.Orders"), "skipped: 0".to_string()]);
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
    // when the usual schema completes that name, `dbo`, `PUBLIC`, `main`, `public` or `default`.
    let scripts = [
        ("tsql", "tests/scripts/tsql/10-defaults.sql", "Sales", 3),
        ("snowflake", "tests/scripts/snowflake/03-defaults.sql", "SALES", 2),
        ("duckdb", "tests/scripts/duckdb/02-defaults.sql", "sales", 2),
        ("postgresql", "tests/scripts/postgresql/02-defaults.sql", "sales", 2),
        ("redshift", "tests/scripts/redshift/02-defaults.sql", "sales", 2),
        ("databricks", "tests/scripts/databricks/02-defaults.sql", "sales", 2),
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

/// The path of the migration script `name`.
fn migration(name: &str) -> String {
    format!("tests/scripts/tsql/migration/{name}.sql")
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
fn graph_json_lists_writes_and_findings() {
    let [a, c, d] = ["a-load", "c-base", "d-redefine"].map(migration);
    let (refused, cp1252) = ("tests/scripts/tsql/08-refused.sql", "tests/scripts/tsql/14-cp1252.sql");
    let mut graph = printed_json(&["graph", "--dialect", "tsql", "--json", &a, &c, &d, refused, cp1252]);
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
    assert_eq!(graph, json!({"writes": writes, "findings": findings, "skipped": 4}));
}

#[test]
fn a_declaration_breaking_the_catalogs_rule_is_refused() {
    // A list with no column, and one that holds `Id` twice, as T-SQL compares names.
    let path = "tests/scripts/tsql/17-declarations.sql";
    let found = tsql(&[path]);
    assert!(found[1].starts_with(&format!("refused: {path}:1:1 ")) && found[1].contains("no column"), "{found:?}");
    assert!(found[2].starts_with(&format!("refused: {path}:3:1 ")) && found[2].contains("twice"), "{found:?}");
}

#[test]
fn a_relation_declared_twice_is_reported() {
    // `dbo.Stage`, then `[dbo].[Stage]`: one relation, declared again, the later kept.
    let path = "tests/scripts/tsql/17-declarations.sql";
    let found = tsql(&[path]);
    assert_eq!(found[0], format!("{path}:9:1 dbo.vStage <- dbo.Stage"));
    assert_eq!(found[3..], [format!("declared_twice: {path}:7:1 [dbo].[Stage]"), "skipped: 0".to_string()]);
    let note = format!("[dbo].[Stage].[Note] -> dbo.vStage.[Note] copy {path}:9:1");
    assert_eq!(walked("trace", "dbo.vStage.Note", &[path]), [note]);
}

/// The lines that `graph` prints for the script of T-SQL's tables.
fn tables() -> Vec<String> {
    tsql(&["tests/scripts/tsql/18-storage.sql"])
}

#[test]
fn tsql_storage_clauses_are_ignored() {
    // SSMS's table with `ROWGUIDCOL`, `FILESTREAM` and `SPARSE` in its columns, and `ON`,
    // `TEXTIMAGE_ON`, `FILESTREAM_ON` and `WITH` after them: declared, not refused, not skipped.
    let found = tables();
    assert!(!found.iter().any(|line| line.starts_with("refused: tests/scripts/tsql/18-storage.sql:1:1 ")), "{found:?}");
    assert_eq!(found.last().map(String::as_str), Some("skipped: 0"));
}

cases! {
    // `ON ps_Day([Day])` is read as polyglot reads it; blanked, its `([Day])` would not parse.
    a_partition_scheme_is_kept:
        lacks(&tables(), |line| line.starts_with("refused: tests/scripts/tsql/18-storage.sql:10:1 "));
}

#[test]
fn a_create_table_as_is_not_blanked() {
    // Its join's `ON` stays, whether a column list or `WITH` follows its name.
    let path = "tests/scripts/tsql/18-storage.sql";
    let found = tables();
    assert_eq!(
        found[..2],
        [
            format!("{path}:12:1 dbo.Joined <- dbo.Other, dbo.Stage"),
            format!("{path}:14:1 dbo.Spread <- dbo.Other, dbo.Stage")
        ]
    );
}

/// The lines that the command `name` prints for `argument`, a column or a write, in the
/// scripts at `paths` in T-SQL.
fn walked(name: &str, argument: &str, paths: &[&str]) -> Vec<String> {
    lines(&[&[name, "--dialect", "tsql", argument], paths].concat())
}

/// What `trace` gives for `column` in the scripts at `paths` in T-SQL, as JSON.
fn traced(column: &str, paths: &[&str]) -> Value {
    printed_json(&[&["trace", "--dialect", "tsql", "--json", column], paths].concat())
}

/// The scripts of a load and its views: three tables, four loads of `dbo.Fact`, the last
/// refused, and four views, the first of which reads the second.
const LINEAGE: &str = "tests/scripts/tsql/lineage";

/// The id of the write at `line` of the load.
fn load(line: usize) -> String {
    format!("{LINEAGE}/2-load.sql:{line}:1")
}

/// The id of the write at `line` of the views.
fn view(line: usize) -> String {
    format!("{LINEAGE}/3-views.sql:{line}:1")
}

#[test]
fn trace_unites_the_edges_of_several_writes() {
    // The first load copies `Amount`, the second derives it: one edge, a copy, of both (R12).
    let found = traced("dbo.Fact.Amount", &[LINEAGE]);
    let edge = json!({
        "parent": {"relation": "dbo.Stage", "column": "Amount"},
        "child": {"relation": "dbo.Fact", "column": "[Amount]"},
        "kind": "copy",
        "writes": [load(1), load(2)],
        "depth": 1,
    });
    assert_eq!(found["edges"][0], edge);
}

#[test]
fn trace_meets_a_declaration_spelled_otherwise() {
    // The view reads `[dbo].[Fact]`, as declared; the loads write `dbo.Fact`: one relation.
    let found = traced("dbo.vBase.Total", &[LINEAGE]);
    assert_eq!(found["edges"][0]["parent"], json!({"relation": "[dbo].[Fact]", "column": "[Amount]"}));
    assert_eq!(found["edges"][1]["child"], json!({"relation": "dbo.Fact", "column": "[Amount]"}));
    assert_eq!(found["edges"][1]["depth"], 2);
}

#[test]
fn trace_gives_gaps_and_refusals_on_the_way() {
    // `vOther` reads a relation that no script declares, which its gap says, so that it is not
    // taken for a base relation; a load of the table that `vBase` reads is refused.
    let found = walked("trace", "dbo.vBoth.Id", &[LINEAGE]);
    let met = [format!("gap: {} unknown_relation dbo.Missing -", view(5)), format!("refused: {} invalid", load(4))];
    assert_eq!(found[found.len() - 2..], met);
}

#[test]
fn trace_follows_a_write_after_its_dependencies() {
    // `vTop` is read before `vBase`, which it reads; answered after it, it has its edge, and no
    // gap.
    let found = walked("trace", "dbo.vTop.Total", &[LINEAGE]);
    let first = format!("dbo.vBase.Total -> dbo.vTop.Total copy {}", view(1));
    assert_eq!(found[..2], [first, format!("  [dbo].[Fact].[Amount] -> dbo.vBase.Total copy {}", view(3))]);
    assert!(!found.iter().any(|line| line.starts_with("gap:")), "{found:?}");
}

#[test]
fn trace_reads_its_argument_under_the_dialect() {
    // T-SQL ignores case, and the command's defaults complete the relation; in Snowflake, `b`
    // means `"B"`, and `"b"` another column.
    let expected = walked("trace", "dbo.vBase.Total", &[LINEAGE]);
    assert_eq!(walked("trace", "DBO.VBASE.total", &[LINEAGE]), expected);
    assert_eq!(lines(&["trace", "--dialect", "tsql", "--schema", "dbo", "vBase.Total", LINEAGE]), expected);
    let path = "tests/scripts/snowflake/04-declared.sql";
    let quoted = [format!("s.\"B\" -> t.\"B\" copy {path}:2:1")];
    assert_eq!(lines(&["trace", "--dialect", "snowflake", "t.\"B\"", path]), quoted);
    assert_eq!(lines(&["trace", "--dialect", "snowflake", "T.b", path]), quoted);
    assert_refused(&command(&["trace", "--dialect", "snowflake", "t.\"b\"", path]), "t.\"b\"");
}

#[test]
fn outline_gives_a_write_in_brief() {
    // A replace write and its answer; a contribute write, refused, its detail text for a person.
    prints("outline-lineage-vBase");
    let refused = walked("outline", &load(4), &[LINEAGE]);
    let fields = [format!("id: {}", load(4)), "target: dbo.Fact".to_string(), "mode: contribute".to_string()];
    assert_eq!(refused[..3], fields);
    assert_eq!(refused[3..5], ["place: 4:1", "columns: -"]);
    assert!(refused[5].starts_with("refused: invalid "), "{refused:?}");
}

#[test]
fn outline_places_clauses() {
    // Outside parentheses and `CASE`: not `CASE`'s `WHEN`, the function `LEFT`, `WITHIN
    // GROUP (ORDER BY ...)`, nor `FROM` in a string; a join at its first word.
    let path = "tests/scripts/tsql/21-clauses.sql";
    let clauses = |id: &str| -> Vec<String> {
        let found = walked("outline", id, &[path]);
        found.into_iter().filter_map(|line| line.strip_prefix("clause: ").map(str::to_string)).collect()
    };
    let view = ["WITH 3:1", "SELECT 4:1", "FROM 7:1", "LEFT OUTER JOIN 8:1", "ON 8:33", "CROSS APPLY 9:1"];
    let view = [view.as_slice(), &["WHERE 10:1", "GROUP BY 11:1", "HAVING 12:1"]].concat();
    assert_eq!(clauses(&format!("{path}:1:1")), view);
    let merge = ["INTO 14:7", "USING 15:1", "ON 15:22", "WHEN 16:1", "SET 16:26", "WHEN 17:1", "VALUES 17:41"];
    assert_eq!(clauses(&format!("{path}:14:1")), merge);
}

#[test]
fn outline_places_clauses_inside_an_existence_check() {
    // The write is at its `EXEC`; its clauses in the file, past the string's start and each
    // doubled quote before them.
    let path = "tests/scripts/tsql/22-existence-quotes.sql";
    let file = read(path);
    let line = file.lines().nth(1).expect("the script's second line");
    let place = |words: &str| format!("clause: {words} 2:{}", line.find(words).expect("the words are on the line") + 1);
    let found = walked("outline", &format!("{path}:2:1"), &[path]);
    assert_eq!(found[3], "place: 2:1");
    let clauses = ["SELECT", "FROM", "WHERE", "GROUP BY"].map(place);
    assert_eq!(found[found.len() - 4..], clauses);
}

#[test]
fn commands_give_json_on_request() {
    let json = |arguments: &[&str]| printed_json(&[arguments, &["--json", LINEAGE]].concat());
    let end = |relation: &str, column: &str| json!({"relation": relation, "column": column});
    let trace = json(&["trace", "--dialect", "tsql", "dbo.vTop.Total"]);
    assert_eq!(trace["column"], end("dbo.vTop", "Total"));
    let first = json!({
        "parent": end("dbo.vBase", "Total"),
        "child": end("dbo.vTop", "Total"),
        "kind": "copy",
        "writes": [view(1)],
        "depth": 1,
    });
    assert_eq!(trace["edges"][0], first);
    assert_eq!(trace["edges"].as_array().map(Vec::len), Some(4));
    assert_eq!((&trace["gaps"], &trace["cycles"]), (&json!([]), &json!([])));
    assert_eq!(trace["refusals"], json!([{"write": load(4), "reason": "invalid"}]));
    let uses = json(&["uses", "--dialect", "tsql", "dbo.vOther.Id"]);
    // The join's condition decides the rows of `Total`; `Id` is selected.
    let both = |column: &str, kind: &str| json!({"parent": end("dbo.vOther", "Id"), "child": end("dbo.vBoth", column), "kind": kind, "writes": [view(7)], "depth": 1});
    assert_eq!(uses["edges"], json!([both("Total", "rows"), both("Id", "copy")]));
    let gap = json!({"write": view(5), "reason": "unknown_relation", "relation": "dbo.Missing", "column": null});
    assert_eq!(json(&["trace", "--dialect", "tsql", "dbo.vOther.Id"])["gaps"], json!([gap]));
    let outline = json(&["outline", "--dialect", "tsql", &view(5)]);
    let clauses = json!([{"keyword": "SELECT", "place": "5:27"}, {"keyword": "FROM", "place": "5:37"}]);
    let answer = json!({
        "write": view(5),
        "columns": ["Id"],
        "edges": [],
        "gaps": [{"reason": "unknown_relation", "relation": "dbo.Missing", "column": null, "detail": "unknown to the catalog"}],
        "refusal": null,
    });
    let expected = json!({"id": view(5), "target": "dbo.vOther", "mode": "replace", "place": "5:1", "clauses": clauses, "answer": answer});
    assert_eq!(outline, expected);
}

#[test]
fn uses_reports_the_scripts_refused_writes() {
    // `dbo.Bad` reads `dbo.O` but does not parse, and takes the views after it with it: no edge
    // is known, and the walk, either way, lists the refused write, which may read the column.
    let path = "tests/scripts/tsql/23-refused-reader.sql";
    for name in ["uses", "trace"] {
        let found = walked(name, "dbo.O.Id", &[path]);
        assert!(matches!(found.as_slice(), [line] if line.starts_with(&format!("refused: {path}:2:1 "))), "{found:?}");
    }
    let walk = printed_json(&["uses", "--dialect", "tsql", "--json", "dbo.O.Id", path]);
    let refused = &walk["refusals"][0];
    assert_eq!((&walk["edges"], &refused["place"]), (&json!([]), &json!(format!("{path}:2:1"))));
    assert!(refused["detail"].is_string() && refused.get("write").is_none(), "{walk}");
    assert_eq!(walk["refusals"].as_array().map(Vec::len), Some(1));
}

#[test]
fn walks_report_dynamic_sql_not_read() {
    // The `EXEC` of a string may read `dbo.O` or write `dbo.vO`: both walks list it after their
    // edges, and the JSON in `not_read`.
    prints("uses-24-dynamic-writer-O-Id");
    prints("trace-24-dynamic-writer-vO-Id");
    let path = "tests/scripts/tsql/24-dynamic-writer.sql";
    let walk = printed_json(&["uses", "--dialect", "tsql", "--json", "dbo.O.Id", path]);
    assert_eq!(walk["not_read"], json!([{"place": format!("{path}:2:1"), "detail": "dynamic SQL"}]));
    assert_eq!(walk["refusals"], json!([]));
}

#[test]
fn readme_shows_what_the_binary_prints() {
    // Each `console` block of the README whose first line runs the binary shows, on its other
    // lines, what that command prints from the repository's root; every command is shown.
    let readme = read("README.md");
    let mut shown = Vec::new();
    for block in readme.split("```console\n").skip(1) {
        let block = block.split("```").next().expect("a console block ends");
        let Some((command, printed)) = transcript(block) else { continue };
        let arguments: Vec<&str> = command.split_whitespace().collect();
        assert_eq!(lines(&arguments), printed.lines().collect::<Vec<_>>(), "{command}");
        shown.extend(arguments.first().copied());
    }
    for verb in ["graph", "trace", "uses", "outline"] {
        assert!(shown.contains(&verb), "the README shows no {verb}");
    }
}
