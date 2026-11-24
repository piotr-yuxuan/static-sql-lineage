// SPDX-License-Identifier: EUPL-1.2
//! `static-sql-lineage`: the column lineage of SQL that writes relations.  How to run it
//! is in `docs/protocol.md`.

mod answer;
mod catalog;
mod command;
mod describe;
mod dialect;
mod edge;
mod graph;
mod lineage;
mod relation;
mod request;
mod script;
mod write;

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use command::{Cli, Command, Verb};
use describe::Description;
use request::Request;

/// The longest request read, in bytes: a project of 6,000 models is a few tens of
/// megabytes.
const LIMIT: u64 = 512 << 20;

fn main() -> ExitCode {
    // A usage error, help and the version are clap's to print, and end the process.
    let output = match Cli::parse() {
        Cli { describe: true, .. } => Ok(serde_json::to_string(&Description::of_build(include_str!("../Cargo.lock")))
            .expect("a description serialises")),
        Cli { verb: None, .. } if std::io::stdin().is_terminal() => {
            let reason = "standard input is a terminal: give a request on it, `-` to read one from it, or a command";
            Cli::command().error(ErrorKind::MissingSubcommand, reason).exit()
        }
        Cli { verb: None | Some(Verb::Request), .. } => Request::read(std::io::stdin().lock(), LIMIT)
            .and_then(Request::respond)
            .map(|response| serde_json::to_string(&response).expect("a response serialises")),
        Cli { verb: Some(Verb::Completions { shell }), .. } => {
            clap_complete::generate(shell, &mut Cli::command(), "static-sql-lineage", &mut std::io::stdout());
            return ExitCode::SUCCESS;
        }
        Cli { verb: Some(verb), .. } => Command::read(verb).and_then(|command| command.run()),
    };
    match output {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("static-sql-lineage: {reason}");
            ExitCode::from(2)
        }
    }
}
