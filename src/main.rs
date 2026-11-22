// SPDX-License-Identifier: EUPL-1.2
//! `sql-lineage`: the column lineage of SQL that writes relations.  How to run it is in
//! `docs/protocol.md`.

mod describe;
mod dialect;

use std::ffi::OsString;
use std::process::ExitCode;

use describe::Description;

fn main() -> ExitCode {
    // An argument need not be Unicode; it is refused like any other.
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    if matches!(arguments.as_slice(), [only] if only == "--describe") {
        let description = Description::of_build(include_str!("../Cargo.lock"));
        println!("{}", serde_json::to_string(&description).expect("a description serialises"));
        return ExitCode::SUCCESS;
    }
    eprintln!("sql-lineage: given {arguments:?}; the only use is --describe");
    ExitCode::from(2)
}
