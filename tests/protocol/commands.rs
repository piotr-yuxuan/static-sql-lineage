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

#[test]
fn a_command_without_a_dialect_is_refused() {
    assert_refused(&graph(&["tests/scripts/tsql"]), "--dialect");
    assert_refused(&graph(&["--dialect", "bigquery", "tests/scripts/tsql"]), "bigquery");
}

#[test]
fn an_unknown_command_is_refused() {
    assert_refused(&run(&["migrate", "--dialect", "tsql", "tests/scripts/tsql"]), "migrate");
}

#[test]
fn an_unknown_option_is_refused() {
    assert_refused(&graph(&["--dialect", "tsql", "--verbose", "tests/scripts/tsql"]), "--verbose");
}

#[test]
fn a_path_that_does_not_exist_is_refused() {
    assert_refused(&graph(&["--dialect", "tsql", "tests/scripts/tsql/none.sql"]), "tests/scripts/tsql/none.sql");
}
