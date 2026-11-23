// SPDX-License-Identifier: EUPL-1.2
//! `sql-lineage`: the column lineage of SQL that writes relations.  How to run it is in
//! `docs/protocol.md`.

mod answer;
mod catalog;
mod command;
mod describe;
mod dialect;
mod edge;
mod graph;
mod relation;
mod request;
mod script;
mod write;

use std::ffi::OsString;
use std::process::ExitCode;

use command::Command;
use describe::Description;
use request::Request;

/// The longest request read, in bytes: a project of 6,000 models is a few tens of
/// megabytes.
const LIMIT: u64 = 512 << 20;

fn main() -> ExitCode {
    // An argument need not be Unicode; it is refused like any other.
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let output = match arguments.as_slice() {
        [] => Request::read(std::io::stdin().lock(), LIMIT)
            .and_then(Request::respond)
            .map(|response| serde_json::to_string(&response).expect("a response serialises")),
        [only] if only == "--describe" => {
            Ok(serde_json::to_string(&Description::of_build(include_str!("../Cargo.lock")))
                .expect("a description serialises"))
        }
        [command, rest @ ..] if command == "graph" => Command::read(rest).and_then(|command| command.run()),
        _ => Err(format!("given {arguments:?}; the uses are --describe, graph, or a request on standard input")),
    };
    match output {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("sql-lineage: {reason}");
            ExitCode::from(2)
        }
    }
}
