// SPDX-License-Identifier: EUPL-1.2
//! The log, as `docs/protocol.md`'s section *Logging* gives it: `-v` and where it is read,
//! and which of it and the two variables wins.

use std::process::Output;

use crate::{assert_succeeded, assert_usage_error, binary, command, fed, request};

/// The binary's output for the command line `arguments`, with `variables` set and a request
/// of one write, `t`, on its standard input.
fn logged(arguments: &[&str], variables: &[(&str, &str)]) -> Output {
    let request = request(&[("s", &["a"])], &[("t", "select a from s")]).to_string();
    fed(binary().args(arguments).envs(variables.iter().copied()), request.as_bytes())
}

/// Status 0, and standard error, which says something.
#[track_caller]
fn said(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(!stderr.is_empty());
    stderr
}

#[test]
fn placement() {
    let graph = ["graph", "--dialect", "duckdb", "examples/tpch"];
    for output in [command(&[&graph[..], &["-v"]].concat()), logged(&["-", "-v"], &[]), logged(&["-v"], &[])] {
        assert!(said(&output).contains("INFO"));
    }
    assert_usage_error(&command(&[&["-v"], &graph[..]].concat()), "'graph'");
    // Refused before standard input is read, so none is given.
    assert_usage_error(&command(&["-v", "-"]), "'-'");
    assert_usage_error(&command(&["--describe", "-v"]), "-v");
    assert_usage_error(&command(&["-v", "--describe"]), "-v");
}

#[test]
fn precedence() {
    let stderr = said(&logged(&["-v"], &[("STATIC_SQL_LINEAGE_LOG", "trace"), ("RUST_LOG", "trace")]));
    assert!(stderr.contains("INFO") && !stderr.contains("DEBUG") && !stderr.contains("TRACE"), "{stderr}");
    assert!(said(&logged(&[], &[("STATIC_SQL_LINEAGE_LOG", "debug"), ("RUST_LOG", "off")])).contains("DEBUG"));
    assert!(said(&logged(&[], &[("RUST_LOG", "debug")])).contains("DEBUG"));
    for silenced in ["off", ""] {
        let output = logged(&[], &[("STATIC_SQL_LINEAGE_LOG", silenced), ("RUST_LOG", "trace")]);
        assert_eq!((output.status.code(), &*String::from_utf8_lossy(&output.stderr)), (Some(0), ""), "{silenced:?}");
    }
}

#[test]
fn levels() {
    let info = said(&logged(&["-v"], &[]));
    assert!(info.contains("dialect=snowflake") && info.contains("writes=1") && !info.contains("answer{id="), "{info}");
    let debug = said(&logged(&["-vv"], &[]));
    assert!(debug.contains("answer{id=t") && debug.contains("answered"), "{debug}");
    let trace = said(&logged(&["-vvv"], &[]));
    assert!(trace.lines().any(|line| line.contains("analyze_query") && line.contains("time.busy")), "{trace}");
    let graph = said(&command(&["graph", "-v", "--dialect", "duckdb", "examples/tpch"]));
    assert!(graph.contains("files=") && graph.contains("writes="), "{graph}");
}

#[test]
fn filter() {
    let stderr = said(&logged(&[], &[("STATIC_SQL_LINEAGE_LOG", "static_sql_lineage::write=trace")]));
    assert!(stderr.lines().all(|line| line.contains("static_sql_lineage::write:")), "{stderr}");
}

#[test]
fn stdout_unchanged() {
    let tpch = ["--dialect", "duckdb", "--json", "examples/tpch"];
    let outline = ["outline", "examples/tpch/reports.sql:8:1"];
    let uses = [&["graph"][..], &["trace", "order_revenue.revenue"], &["uses", "lineitem.l_discount"], &outline];
    let mut pairs: Vec<_> =
        uses.map(|verb| (command(&[verb, &tpch].concat()), command(&[verb, &["-vvv"], &tpch].concat()))).into();
    pairs.push((logged(&[], &[]), logged(&["-vvv"], &[])));
    for (quiet, loud) in pairs {
        assert_succeeded(&quiet);
        said(&loud);
        assert_eq!(String::from_utf8_lossy(&quiet.stdout), String::from_utf8_lossy(&loud.stdout));
    }
}
