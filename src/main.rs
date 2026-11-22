//! `sql-lineage`: the column lineage of SQL that writes relations.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    eprintln!("sql-lineage: given {arguments:?}; nothing is built yet");
    ExitCode::from(2)
}
