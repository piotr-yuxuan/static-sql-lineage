// SPDX-License-Identifier: EUPL-1.2
//! The log, as `docs/protocol.md`'s section *Logging* gives it: `-v` and where it is read,
//! and which of it and the two variables wins.

use std::process::Output;

use crate::{assert_usage_error, binary, command, fed, request};

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
    assert_usage_error(&logged(&["-v", "-"], &[]), "'-'");
    assert_usage_error(&logged(&["--describe", "-v"], &[]), "-v");
    assert_usage_error(&logged(&["-v", "--describe"], &[]), "-v");
}

#[test]
fn precedence() {
    let stderr = said(&logged(&["-v"], &[("STATIC_SQL_LINEAGE_LOG", "trace"), ("RUST_LOG", "trace")]));
    assert!(stderr.contains("INFO") && !stderr.contains("DEBUG") && !stderr.contains("TRACE"), "{stderr}");
    assert!(said(&logged(&[], &[("STATIC_SQL_LINEAGE_LOG", "debug"), ("RUST_LOG", "off")])).contains("INFO"));
    assert!(said(&logged(&[], &[("RUST_LOG", "debug")])).contains("INFO"));
    for silenced in ["off", ""] {
        let output = logged(&[], &[("STATIC_SQL_LINEAGE_LOG", silenced), ("RUST_LOG", "trace")]);
        assert_eq!((output.status.code(), &*String::from_utf8_lossy(&output.stderr)), (Some(0), ""), "{silenced:?}");
    }
}
