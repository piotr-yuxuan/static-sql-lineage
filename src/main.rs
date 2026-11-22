//! `sql-lineage`: the column lineage of SQL that writes relations.  How to run it is in
//! `docs/protocol.md`.

mod describe;
mod dialect;

use std::process::ExitCode;

use describe::Description;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments == ["--describe"] {
        let description = Description::of_build(include_str!("../Cargo.lock"));
        println!("{}", serde_json::to_string(&description).expect("a description serialises"));
        return ExitCode::SUCCESS;
    }
    eprintln!("sql-lineage: given {arguments:?}; the only use is --describe");
    ExitCode::from(2)
}
