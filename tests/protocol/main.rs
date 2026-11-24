// SPDX-License-Identifier: EUPL-1.2
//! The binary run as a caller runs it, as `docs/protocol.md` gives its uses: here
//! `--describe` and the arguments it refuses, and the helpers of the modules.

/// One test per row, `name: body;`, named after the case: a scenario's identifier, `.`
/// and `-` becoming `_`. The row's attributes, such as `ignore`, pass through.
macro_rules! cases {
    ($($(#[$attribute:meta])* $name:ident: $body:expr;)*) => {
        $(
            #[test]
            $(#[$attribute])*
            fn $name() {
                $body;
            }
        )*
    };
}

mod agreement;
mod answers;
mod cases;
mod catalogue;
mod commands;
mod edges;
mod requests;
mod scenarios;
mod stars;
mod statements;

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

/// The command that runs the built binary.
fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_static-sql-lineage"))
}

fn run<S: AsRef<OsStr>>(arguments: &[S]) -> Output {
    binary().args(arguments).output().expect("the binary runs")
}

/// The binary's output for the command line `arguments`, run from the repository's root.
fn command(arguments: &[&str]) -> Output {
    binary().current_dir(env!("CARGO_MANIFEST_DIR")).args(arguments).output().expect("the binary runs")
}

/// The lines that the command line `arguments` prints, which the binary does not refuse.
fn lines(arguments: &[&str]) -> Vec<String> {
    let output = command(arguments);
    assert_succeeded(&output);
    String::from_utf8(output.stdout).expect("the output is UTF-8").lines().map(str::to_string).collect()
}

/// The JSON that the command line `arguments` prints, as [`lines`] reads it.
fn printed_json(arguments: &[&str]) -> Value {
    serde_json::from_str(&lines(arguments).join("\n")).expect("the output is JSON")
}

/// The binary's output for `input` on its standard input.
fn answers(input: &[u8]) -> Output {
    let mut child = (binary().stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()))
        .spawn()
        .expect("the binary runs");
    child.stdin.take().expect("a pipe to the binary").write_all(input).expect("the request is written");
    child.wait_with_output().expect("the binary ends")
}

/// The answers to `request`, which the binary does not refuse.
fn respond(request: &Value) -> Vec<Value> {
    let output = answers(request.to_string().as_bytes());
    assert_succeeded(&output);
    let response: Value = serde_json::from_slice(&output.stdout).expect("the response is JSON");
    response["answers"].as_array().expect("the response has answers").clone()
}

/// A request in `dialect` of `catalog`, each relation with its columns, and of
/// `writes`, each a target, which is also its id, and its query.
fn request_in(dialect: &str, catalog: &[(&str, &[&str])], writes: &[(&str, &str)]) -> Value {
    let catalog: Vec<Value> =
        catalog.iter().map(|(relation, columns)| json!({"relation": relation, "columns": columns})).collect();
    let writes: Vec<Value> =
        writes.iter().map(|(target, sql)| json!({"id": target, "target": target, "sql": sql})).collect();
    json!({"protocol": 1, "dialect": dialect, "catalog": catalog, "writes": writes})
}

/// A request in Snowflake, as [`request_in`] builds one.
fn request(catalog: &[(&str, &[&str])], writes: &[(&str, &str)]) -> Value {
    request_in("snowflake", catalog, writes)
}

/// An answer in short: each edge as `parent -> child kind`, each gap as
/// `gap reason relation column`, `-` for none, and its refusal as `refused reason`.
fn outline(answer: &Value) -> Vec<String> {
    let text = |value: &Value| value.as_str().unwrap_or("-").to_string();
    let end = |end: &Value| format!("{}.{}", text(&end["relation"]), text(&end["column"]));
    let all = |field: &str| answer[field].as_array().cloned().unwrap_or_default();
    let edges = all("edges")
        .into_iter()
        .map(|edge| format!("{} -> {} {}", end(&edge["parent"]), end(&edge["child"]), text(&edge["kind"])));
    let gaps = (all("gaps").into_iter())
        .map(|gap| format!("gap {} {} {}", text(&gap["reason"]), text(&gap["relation"]), text(&gap["column"])));
    let refusal = answer["refusal"]["reason"].as_str().map(|reason| format!("refused {reason}"));
    edges.chain(gaps).chain(refusal).collect()
}

/// The outline of each answer to `request`.
fn outlines(request: &Value) -> Vec<Vec<String>> {
    respond(request).iter().map(outline).collect()
}

/// The outline of the answer to `sql` against `catalog` in `dialect`, its target `t`.
fn trace_in(dialect: &str, catalog: &[(&str, &[&str])], sql: &str) -> Vec<String> {
    outline(&respond(&request_in(dialect, catalog, &[("t", sql)]))[0])
}

/// The outline of the answer to `sql` against `catalog` in Snowflake, its target `t`.
fn trace(catalog: &[(&str, &[&str])], sql: &str) -> Vec<String> {
    trace_in("snowflake", catalog, sql)
}

/// Asserts that the outline of the answer to `sql` against `catalog` in `dialect`, its
/// target `t`, is `expected`.
#[track_caller]
fn traces_in(dialect: &str, catalog: &[(&str, &[&str])], sql: &str, expected: &[&str]) {
    assert_eq!(trace_in(dialect, catalog, sql), expected, "{sql}");
}

/// Asserts that the outline of the answer to `sql` against `catalog` in Snowflake, its
/// target `t`, is `expected`.
#[track_caller]
fn traces(catalog: &[(&str, &[&str])], sql: &str, expected: &[&str]) {
    traces_in("snowflake", catalog, sql, expected);
}

/// Asserts that the answer to `sql` against `catalog` in Snowflake, its target `t`, gives
/// the columns `columns` and the outline `expected`.
#[track_caller]
fn answered(catalog: &[(&str, &[&str])], sql: &str, columns: Value, expected: &[&str]) {
    let answer = &respond(&request(catalog, &[("t", sql)]))[0];
    assert_eq!(answer["columns"], columns, "{sql}");
    assert_eq!(outline(answer), expected, "{sql}");
}

/// Asserts that no line of the outline `found` is `unwanted`, as a reject scenario says.
#[track_caller]
fn lacks(found: &[String], unwanted: impl Fn(&str) -> bool) {
    assert!(!found.iter().any(|line| unwanted(line)), "{found:?}");
}

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).expect("a file of the repository reads")
}

fn description() -> Value {
    let output = run(&["--describe"]);
    assert_succeeded(&output);
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

/// A use that succeeds: status 0, and nothing on standard error.
fn assert_succeeded(output: &Output) {
    assert_eq!((output.status.code(), &*String::from_utf8_lossy(&output.stderr)), (Some(0), ""));
}

/// A refusal: status 2, nothing on standard output, one line on standard error that
/// contains `naming`.
#[track_caller]
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
    assert_eq!(description["tool"], "static-sql-lineage");
    assert_eq!(description["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(description["protocols"], json!([1]));
    assert_eq!(description["engine"]["name"], "polyglot-sql");
    // Cargo.toml pins the engine at =0.14.3, from crates.io.
    assert_eq!(description["engine"]["version"], "0.14.3");
    assert!(description["engine"]["source"].as_str().is_some_and(|source| source.starts_with("registry+")));
    // Both verbatim, as the engine's own entry in Cargo.lock gives them.
    let lock = read("Cargo.lock");
    let entry = lock.split("[[package]]").find(|package| package.contains("name = \"polyglot-sql\"\n"));
    let entry = entry.expect("Cargo.lock has an entry for polyglot-sql");
    assert!(entry.contains(&format!("version = {}\n", description["engine"]["version"])), "{entry}");
    assert!(entry.contains(&format!("source = {}\n", description["engine"]["source"])), "{entry}");
    assert_eq!(description["dialects"], json!(["snowflake", "tsql", "duckdb"]));
}

#[test]
fn describe_follows_the_protocol_document() {
    let document = read("docs/protocol.md");
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
fn refuses_describe_with_another_argument() {
    assert_refused(&run(&["--describe", "--explain"]), "--explain");
}

#[cfg(unix)]
#[test]
fn refuses_an_argument_that_is_not_unicode() {
    use std::os::unix::ffi::OsStrExt;
    assert_refused(&run(&[OsStr::from_bytes(b"\xff")]), "\\xFF");
}
