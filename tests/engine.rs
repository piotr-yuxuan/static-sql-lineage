// SPDX-License-Identifier: EUPL-1.2
//! The engine's suite: one row per issue of polyglot-sql that bears on the answers, named
//! after it, which calls the engine, never the binary, with the issue's reproduction and
//! the behaviour that the issue states as expected; ignored while the issue is open. A
//! fact that the engine has no variant for yet is read through its `Debug` text.

use DialectType::{Databricks, DuckDB, PostgreSQL, Redshift, Snowflake, TSQL};
use polyglot_sql::expressions::Expression;
use polyglot_sql::scope::SourceKind;
use polyglot_sql::validation::{SchemaValidationOptions, validate_with_schema};
use polyglot_sql::{
    AnalyzeQueryOptions, ColumnReferenceFact, ColumnUseContext, DialectType, QueryAnalysis, TransformKind as Transform,
    ValidationSchema, ValidationSeverity, analyze_query, generate, parse_one,
};
use serde_json::json;

#[macro_use]
#[path = "protocol/rows.rs"]
mod rows;

/// The table of most issues' reproductions.
const T: &[(&str, &[&str])] = &[("s.t", &["k", "status", "amount", "c"])];
/// The tables of the reproductions of #562 and #563.
const TU: &[(&str, &[&str])] = &[("s.t", &["k", "a"]), ("s.u", &["k", "x"])];
/// The table of the reproductions of #564, #565 and #568.
const KAB: &[(&str, &[&str])] = &[("s.t", &["k", "a", "b VARCHAR"])];
/// The table of the reproductions of #566 and #567.
const KABI: &[(&str, &[&str])] = &[("s.t", &["k", "a", "b VARCHAR", "items VARIANT"])];

/// A schema of `tables`, each `schema.table` or `table` with its columns, each `name` or
/// `name TYPE`, `INT` by default.
fn schema(tables: &[(&str, &[&str])]) -> ValidationSchema {
    let column = |column: &&str| {
        let (name, kind) = column.split_once(' ').unwrap_or((column, "INT"));
        json!({"name": name, "type": kind})
    };
    let table = |(relation, columns): &(&str, &[&str])| {
        let (schema, name) = relation.rsplit_once('.').map_or((None, *relation), |(schema, name)| (Some(schema), name));
        json!({"name": name, "schema": schema, "columns": columns.iter().map(column).collect::<Vec<_>>()})
    };
    serde_json::from_value(json!({"tables": tables.iter().map(table).collect::<Vec<_>>()})).expect("a schema")
}

/// The analysis of `sql` in `dialect` against `tables`, as [`schema`] reads them.
fn analysis(dialect: DialectType, tables: &[(&str, &[&str])], sql: &str) -> QueryAnalysis {
    let options = AnalyzeQueryOptions { dialect, schema: Some(schema(tables)), ..Default::default() };
    analyze_query(sql, options).unwrap_or_else(|error| panic!("{sql}: {error}"))
}

/// The codes of the errors of `sql` in `dialect` against `tables`, every optional check on;
/// none for valid SQL.
fn checked(dialect: DialectType, tables: &[(&str, &[&str])], sql: &str) -> Vec<String> {
    let options =
        SchemaValidationOptions { semantic: true, check_references: true, check_types: true, ..Default::default() };
    let errors = validate_with_schema(sql, dialect, &schema(tables), &options).errors.into_iter();
    errors.filter(|error| error.severity == ValidationSeverity::Error).map(|error| error.code).collect()
}

/// The output columns' names.
fn names(analysis: &QueryAnalysis) -> Vec<String> {
    analysis.projections.iter().map(|p| p.name.clone().unwrap_or_default()).collect()
}

/// The upstream of the output column at `at`, each `source.column` in lower case, sorted.
fn upstream(analysis: &QueryAnalysis, at: usize) -> Vec<String> {
    named(analysis.projections[at].upstream.iter())
}

/// Each of `references` as `source.column` in lower case, sorted.
fn named<'a>(references: impl Iterator<Item = &'a ColumnReferenceFact>) -> Vec<String> {
    let name = |r: &ColumnReferenceFact| format!("{}.{}", r.source_name.as_deref().unwrap_or("?"), r.column);
    let mut found: Vec<String> = references.map(|r| name(r).to_lowercase()).collect();
    found.sort();
    found
}

/// Each use as the issues print it, `Context: expression`, the expression in lower case.
fn uses(analysis: &QueryAnalysis) -> Vec<String> {
    analysis.column_uses.iter().map(|u| format!("{:?}: {}", u.context, u.expression_sql.to_lowercase())).collect()
}

cases! {
    polyglot_556: {
        for function in ["object_construct(*)", "hash(*)"] {
            let found = analysis(Snowflake, T, &format!("select k, {function} as obj from s.t"));
            assert_eq!(upstream(&found, 1), ["s.t.amount", "s.t.c", "s.t.k", "s.t.status"], "{function}");
            assert!(found.projections[1].upstream.iter().all(|r| r.path_transform_kind == Some(Transform::Expression)));
        }
        assert!(upstream(&analysis(Snowflake, T, "select k, count(*) as n from s.t group by k"), 1).is_empty());
    };
    polyglot_557: for sql in [
        "select t.k, f.value as v from s.t t, lateral flatten(input => t.items) f",
        "select t.k, f.value as v from s.t t, table(flatten(input => t.items)) f",
        "select t.k, f.value as v from s.t t cross join table(flatten(input => t.items)) f",
        "select s.value from table(split_to_table('x,y', ',')) s",
    ] {
        assert_eq!(checked(Snowflake, &[("s.t", &["k", "a", "items VARIANT"])], sql), [""; 0], "{sql}");
    };
    polyglot_558: for (sql, name) in [("select id from s.t", "ID"), ("select \"id\" from s.t", "id")] {
        let found = analysis(Snowflake, &[("S.T", &["ID", "\"id\""])], sql);
        assert_eq!([&names(&found)[0], &found.projections[0].upstream[0].column], [name; 2], "{sql}");
    };
    polyglot_559: {
        let sql = "select * from (select k, status, amount from s.t) \
                   pivot (sum(amount) for status in ('placed', 'shipped'))";
        assert!(uses(&analysis(Snowflake, T, sql)).contains(&"Group: k".to_string()));
    };
    // The row pins 0.14.4's form of the fix, `Filter` uses, which the issue left open.
    polyglot_560: {
        let found = analysis(Snowflake, T, "select k, metric, val from s.t unpivot (val for metric in (amount, c))");
        assert!(upstream(&found, 1).is_empty());
        assert_eq!(uses(&found), ["Filter: amount", "Filter: c"]);
        let sql = "select k, metric, val from s.t unpivot include nulls (val for metric in (amount, c))";
        assert!(uses(&analysis(Snowflake, T, sql)).is_empty());
    };
    polyglot_561: {
        for clause in ["", " rename (a as z)", " replace (a + 1 as a)"] {
            assert!(parse_one(&format!("select * ilike '%a%'{clause} from s.t"), Snowflake).is_ok(), "{clause}");
        }
        let tables: &[(&str, &[&str])] = &[("s.t", &["k", "a", "b", "amount"])];
        for (pattern, columns) in [("%a%", &["A", "AMOUNT"][..]), ("a_ount", &["AMOUNT"])] {
            assert_eq!(names(&analysis(Snowflake, tables, &format!("select * ilike '{pattern}' from s.t"))), columns);
        }
    };
    // The row pins 0.14.4's form of the fix, the projection's step in `scope_path`, one of
    // the two that the issue offered.
    polyglot_562: for (at, sql) in [
        (1, "select k, (select max(u.x) from s.u u where u.k = t.k) as top, a from s.t t"),
        (2, "select k, a, (select max(u.x) from s.u u where u.k = t.k) as top from s.t t"),
    ] {
        let paths: Vec<String> = analysis(Snowflake, TU, sql).column_uses.into_iter().map(|u| u.scope_path).collect();
        assert_eq!(paths, [format!("root.expressions[{at}].subqueries[0]")], "{sql}");
    };
    polyglot_563: for (join, sides, kind) in [
        ("left", &["s.t.k"][..], Transform::Direct),
        ("right", &["s.u.k"], Transform::Direct),
        ("full", &["s.t.k", "s.u.k"], Transform::Expression),
    ] {
        let found = analysis(Snowflake, TU, &format!("select k from s.t {join} join s.u using (k)"));
        assert_eq!(upstream(&found, 0), sides, "{join}");
        assert!(found.projections[0].upstream.iter().all(|r| r.path_transform_kind == Some(kind)), "{join}");
        let joined = found.column_uses.iter().filter(|u| u.context == ColumnUseContext::Join);
        assert!(named(joined.flat_map(|u| &u.references).map(|r| &r.reference)).contains(&"s.u.k".into()), "{join}");
    };
    polyglot_564: for sql in [
        "select listagg(b, ',') from s.t",
        "select listagg(b, ',') within group (order by a) from s.t",
        "select k, listagg(b, ',') within group (order by a) from s.t group by k",
    ] {
        assert_eq!(checked(Snowflake, KAB, sql), [""; 0], "{sql}");
    };
    polyglot_565: for grouping in ["k", "all"] {
        let sql = format!("select k, count(*) from s.t group by {grouping} having a > 0");
        assert!(checked(Snowflake, KAB, &sql).contains(&"E230".into()), "{sql}");
    };
    #[ignore = "tobilg/polyglot#566"]
    polyglot_566: {
        let names_of = |sql| names(&analysis(Snowflake, KABI, sql));
        assert_eq!(names_of("select a * 2, upper(b), a, a from s.t"), ["A * 2", "UPPER(B)", "A", "A_1"]);
        let pivot = "select * from (select k, b, a from s.t) pivot (sum(a) for b in ('x', 'y'))";
        assert_eq!(names_of(pivot), ["K", "'x'", "'y'"]);
    };
    polyglot_567: for (sql, columns) in [
        ("select x.* from s.t t, lateral split_to_table(t.b, ',') x", &["SEQ", "INDEX", "VALUE"][..]),
        ("select f.* from s.t t, lateral flatten(input => t.items) f",
            &["SEQ", "KEY", "PATH", "INDEX", "VALUE", "THIS"]),
    ] {
        assert_eq!(names(&analysis(Snowflake, KABI, sql)), columns, "{sql}");
    };
    polyglot_568: {
        assert_eq!(checked(Snowflake, KAB, "select k from s.t where k in (select k from s.t)"), [""; 0]);
        let sql = "select k from s.t where k in (select k, a from s.t)";
        assert!(checked(Snowflake, KAB, sql).contains(&"E217".into()));
    };
    #[ignore = "tobilg/polyglot#569"]
    polyglot_569: for (key, valid) in [("id", true), ("amount", false), ("missing_column", false)] {
        let tables: &[(&str, &[&str])] = &[("orders", &["id", "amount"]), ("buyers", &["id"])];
        let sql = format!("select o.amount from orders o join buyers b using ({key})");
        assert_eq!(checked(Databricks, tables, &sql).is_empty(), valid, "{sql}");
    };
    #[ignore = "tobilg/polyglot#573"]
    polyglot_573: for (dialect, sql) in [
        (Snowflake, "select o.a from s.t as O"),
        (DuckDB, "select o.a from s.t as O"),
        (PostgreSQL, "select o.a from s.t as O"),
        (Redshift, "select \"X\".a from s.t as x"),
    ] {
        assert_eq!(upstream(&analysis(dialect, &[("s.t", &["a"])], sql), 0), ["s.t.a"], "{dialect:?}: {sql}");
    };
    #[ignore = "tobilg/polyglot#574"]
    polyglot_574: {
        let tables: &[(&str, &[&str])] = &[("s.t", &["a", "b"]), ("s.u", &["x", "c"])];
        let found = analysis(Snowflake, tables, "select a from s.t where (a, b) in (select x, c from s.u)");
        let references = found.column_uses.iter().flat_map(|u| &u.references).map(|r| &r.reference);
        assert!(named(references).contains(&"s.u.c".into()));
    };
    #[ignore = "tobilg/polyglot#575"]
    polyglot_575: {
        let sql = "select a, (select t.b + max(u.x) from s.u u where u.k = t.k) as m from s.t t";
        let tables: &[(&str, &[&str])] = &[("s.t", &["k", "a", "b"]), ("s.u", &["k", "x"])];
        assert_eq!(upstream(&analysis(Snowflake, tables, sql), 1), ["s.t.b", "s.u.x"]);
    };
    #[ignore = "tobilg/polyglot#576"]
    polyglot_576: {
        let tables: &[(&str, &[&str])] = &[("s.t", &["a"]), ("s.c", &["z"])];
        let cte = "with c as (select a from s.t) select";
        assert_ne!(upstream(&analysis(Snowflake, tables, &format!("{cte} a from \"c\"")), 0), ["s.t.a"]);
        let found = analysis(Snowflake, tables, &format!("{cte} z from \"c\""));
        assert!(found.projections[0].upstream.iter().all(|r| r.source_kind != SourceKind::Cte));
    };
    #[ignore = "tobilg/polyglot#577"]
    polyglot_577: for sql in ["select total = a from t", "select total = a + 1 from t"] {
        let Ok(Expression::Select(select)) = parse_one(sql, TSQL) else { panic!("{sql} is no select") };
        assert!(matches!(&select.expressions[0], Expression::Alias(alias) if alias.alias.name == "total"), "{sql}");
    };
    #[ignore = "tobilg/polyglot#578"]
    polyglot_578: for sql in [
        "insert t (k, a) select k, x from u",
        "delete from t from t join u on t.k = u.k",
        "alter table t nocheck constraint ck_a",
        "alter table t check constraint ck_a",
        "alter table t with check check constraint ck_a",
    ] {
        assert!(parse_one(sql, TSQL).is_ok(), "{sql}");
    };
    #[ignore = "tobilg/polyglot#579"]
    polyglot_579: for sql in [
        "create table t (a int, index ix_a nonclustered (a))",
        "create table t (a int index ix_a nonclustered)",
        "create table t (a int not null, constraint pk primary key nonclustered hash (a) with (bucket_count = 8)) \
         with (memory_optimized = on)",
        "create table t (a varchar(10) masked with (function = 'default()'))",
        "create table t (a varchar(10) collate Latin1_General_BIN2 encrypted with (column_encryption_key = k1, \
         encryption_type = deterministic, algorithm = 'AEAD_AES_256_CBC_HMAC_SHA_256'))",
        "create table t (a int, b varchar(max)) on [primary] textimage_on [primary]",
        "create table t (id uniqueidentifier rowguidcol not null)",
    ] {
        assert!(parse_one(sql, TSQL).is_ok(), "{sql}");
    };
    #[ignore = "tobilg/polyglot#580"]
    polyglot_580: for (dialect, sql) in [
        (Snowflake, "create table t (a int) cluster by (a) comment = 'x'"),
        (Snowflake, "create table t (a varchar with masking policy p)"),
        (PostgreSQL, "create table t (a int) tablespace ts"),
        (PostgreSQL, "create table t (a int) with (fillfactor = 70) tablespace ts"),
        (PostgreSQL, "create temp table t (a int) on commit drop"),
        (Redshift, "create table t (a int, b int) interleaved sortkey (a, b)"),
        (Databricks, "create table t (a int, b string) using delta partitioned by (a) comment 'z'"),
        (Databricks, "create table t (a int, b string) using delta partitioned by (a) tblproperties ('x' = 'y') \
                      comment 'z'"),
    ] {
        assert!(parse_one(sql, dialect).is_ok(), "{dialect:?}: {sql}");
    };
    #[ignore = "tobilg/polyglot#581"]
    polyglot_581: {
        let parsed = parse_one("select N'it''s' as a", TSQL).expect("it parses");
        let generated = generate(&parsed, TSQL).expect("it generates");
        assert!(generated.contains("N'it''s'") && parse_one(&generated, TSQL).is_ok(), "{generated}");
    };
    #[ignore = "tobilg/polyglot#582"]
    polyglot_582: {
        let sql = "select o.k, f as item from s.t o, o.items f";
        let found = analysis(Redshift, &[("s.t", &["k", "items SUPER"])], sql);
        assert_eq!(upstream(&found, 1), ["s.t.items"]);
        assert!(!found.relations.iter().any(|relation| relation.name == "o.items"));
    };
    #[ignore = "tobilg/polyglot#583"]
    polyglot_583: {
        let tables: &[(&str, &[&str])] = &[("s.t", &["k", "a", "b"])];
        assert_eq!(uses(&analysis(Snowflake, tables, "select distinct a, b from s.t")), ["Distinct: a", "Distinct: b"]);
        assert_eq!(uses(&analysis(DuckDB, tables, "select distinct on (k) a, b from s.t")), ["Distinct: k"]);
        let sql = "select a from (select distinct a, b from s.t) as d";
        assert_eq!(uses(&analysis(DuckDB, tables, sql)), ["Distinct: a", "Distinct: b"]);
    };
}
