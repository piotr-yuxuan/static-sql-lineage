//! `--describe` and the uses the binary refuses, as `docs/protocol.md` gives them.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sql-lineage")).args(arguments).output().expect("the binary runs")
}

fn description() -> Value {
    let output = run(&["--describe"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("the description is JSON")
}

/// The paths of every key in `value`, such as `engine.name`.
fn keys(value: &Value, prefix: &str, into: &mut BTreeSet<String>) {
    if let Value::Object(fields) = value {
        for (key, field) in fields {
            let path = format!("{prefix}{key}");
            keys(field, &format!("{path}."), into);
            into.insert(path);
        }
    }
}

/// A refusal: status 2, nothing on standard output, one line on standard error that
/// contains `naming`.
fn assert_refused(output: &Output, naming: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(output.stdout.is_empty(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    assert_eq!(stderr.lines().count(), 1, "stderr: {stderr}");
    assert!(stderr.contains(naming), "stderr does not name {naming}: {stderr}");
}

#[test]
fn describe_names_the_build() {
    let description = description();
    assert_eq!(description["tool"], "sql-lineage");
    assert_eq!(description["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(description["protocols"], json!([]));
    assert_eq!(description["engine"]["name"], "polyglot-sql");
    // Cargo.toml pins the engine at =0.14.3, from crates.io.
    assert_eq!(description["engine"]["version"], "0.14.3");
    assert!(description["engine"]["source"].as_str().is_some_and(|source| source.starts_with("registry+")));
    assert_eq!(description["dialects"], json!(["snowflake", "tsql", "duckdb"]));
}

#[test]
fn describe_follows_the_protocol_document() {
    let document = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/protocol.md"))
        .expect("docs/protocol.md reads");
    let section = document.split("## `--describe`").nth(1).expect("docs/protocol.md has a section on --describe");
    let example = section.split("```json").nth(1).and_then(|rest| rest.split("```").next()).expect("an example");
    let (mut documented, mut described) = (BTreeSet::new(), BTreeSet::new());
    keys(&serde_json::from_str(example).expect("the example is JSON"), "", &mut documented);
    keys(&description(), "", &mut described);
    assert_eq!(documented, described);
}

#[test]
fn refuses_an_argument_other_than_describe() {
    assert_refused(&run(&["--explain"]), "--explain");
}

#[test]
fn refuses_no_argument() {
    assert_refused(&run(&[]), "[]");
}
