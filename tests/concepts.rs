//! The concept check: every type in `src/` has a home in `docs/concepts.md`, and every
//! home names a type.  A type is a struct, an enum, a trait or a type alias outside
//! `#[cfg(test)]`, whatever its visibility; a home is a name in backticks on a `home:`
//! line, or in the Home cell of a table's row.

use std::collections::BTreeSet;
use std::path::Path;

/// What the check finds wrong.
#[derive(Debug, Default, PartialEq)]
struct Undeclared {
    /// The types that no home names.
    types: BTreeSet<String>,
    /// The names in a home that are no type.
    homes: BTreeSet<String>,
}

/// The types of `sources` that no home of `concepts` names, and the homes that name no
/// type.
fn undeclared(sources: &[String], concepts: &str) -> Undeclared {
    let mut types = BTreeSet::new();
    for source in sources {
        collect(&syn::parse_file(source).expect("a source file parses").items, &mut types);
    }
    let mut homes = BTreeSet::new();
    let mut home_column = None;
    for line in concepts.lines() {
        if let Some(home) = line.strip_prefix("- home:") {
            homes.extend(backticked(home));
            continue;
        }
        let cells: Vec<&str> =
            line.strip_prefix('|').map(|row| row.split('|').map(str::trim).collect()).unwrap_or_default();
        if cells.is_empty() {
            home_column = None;
        } else if let Some(column) = cells.iter().position(|cell| *cell == "Home") {
            home_column = Some(column);
        } else if let Some(cell) = home_column.and_then(|column| cells.get(column)) {
            homes.extend(backticked(cell));
        }
    }
    Undeclared {
        types: types.difference(&homes).cloned().collect(),
        homes: homes.difference(&types).cloned().collect(),
    }
}

/// Adds to `types` the types that `items` define, in inline modules too, except under
/// `#[cfg(test)]`.
fn collect(items: &[syn::Item], types: &mut BTreeSet<String>) {
    for item in items {
        let (attrs, name, inner) = match item {
            syn::Item::Struct(item) => (&item.attrs, Some(&item.ident), None),
            syn::Item::Enum(item) => (&item.attrs, Some(&item.ident), None),
            syn::Item::Trait(item) => (&item.attrs, Some(&item.ident), None),
            syn::Item::Type(item) => (&item.attrs, Some(&item.ident), None),
            syn::Item::Mod(item) => (&item.attrs, None, item.content.as_ref().map(|(_, items)| items)),
            _ => continue,
        };
        let test_only = attrs
            .iter()
            .any(|attr| attr.path().is_ident("cfg") && attr.parse_args::<syn::Ident>().is_ok_and(|arg| arg == "test"));
        if test_only {
            continue;
        }
        types.extend(name.map(ToString::to_string));
        if let Some(inner) = inner {
            collect(inner, types);
        }
    }
}

/// The names in backticks in `text`.
fn backticked(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('`').skip(1).step_by(2).map(str::to_string)
}

/// The repository's sources, and its `docs/concepts.md`.
fn repository() -> (Vec<String>, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = Vec::new();
    let mut directories = vec![root.join("src")];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).expect("a source directory reads") {
            let path = entry.expect("a directory entry reads").path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(std::fs::read_to_string(&path).expect("a source file reads"));
            }
        }
    }
    let concepts = std::fs::read_to_string(root.join("docs/concepts.md")).expect("docs/concepts.md reads");
    (sources, concepts)
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(ToString::to_string).collect()
}

/// Two homes, `Answer` on a `home:` line and `Request` in a Home cell; the backticks of
/// another column name nothing.
const CONCEPTS: &str = "\
## Answer

- home: `Answer`.

| Term | Defined as | Home |
|---|---|---|
| request | one JSON document `on stdin` | `Request` |
| protocol | the number of its format | |
";

#[test]
fn concepts_report_a_type_with_no_home() {
    let found = undeclared(&["struct Answer; struct Request; enum Stray {}".to_string()], CONCEPTS);
    assert_eq!(found, Undeclared { types: names(&["Stray"]), homes: names(&[]) });
}

#[test]
fn concepts_report_a_home_with_no_type() {
    let found = undeclared(&["struct Answer;".to_string()], CONCEPTS);
    assert_eq!(found, Undeclared { types: names(&[]), homes: names(&["Request"]) });
}

#[test]
fn concepts_ignore_associated_types() {
    let source = "struct Answer; struct Request;
        impl std::str::FromStr for Answer { type Err = (); fn from_str(_: &str) -> Result<Self, ()> { Err(()) } }";
    assert_eq!(undeclared(&[source.to_string()], CONCEPTS), Undeclared::default());
}

#[test]
fn concepts_ignore_comments() {
    let source = "// struct Stray;
        /// Says struct Documented;
        struct Answer;
        struct Request;
        const TEXT: &str = \"struct Quoted;\";";
    assert_eq!(undeclared(&[source.to_string()], CONCEPTS), Undeclared::default());
}

#[test]
fn concepts_ignore_test_types() {
    let source = "struct Answer; struct Request;
        #[cfg(test)] struct Helper;
        #[cfg(test)] mod tests { struct Fixture; }";
    assert_eq!(undeclared(&[source.to_string()], CONCEPTS), Undeclared::default());
}

#[test]
fn concepts_find_types_in_inline_modules() {
    let source = "struct Answer; mod inner { pub struct Request; type Stray = u8; }";
    let found = undeclared(&[source.to_string()], CONCEPTS);
    assert_eq!(found, Undeclared { types: names(&["Stray"]), homes: names(&[]) });
}

#[test]
fn every_type_has_a_home() {
    let (sources, concepts) = repository();
    assert!(!sources.is_empty(), "no source file was read");
    let found = undeclared(&sources, &concepts);
    assert!(found.types.is_empty(), "types with no home in docs/concepts.md: {:?}", found.types);
}

#[test]
fn every_home_names_a_type() {
    let (sources, concepts) = repository();
    assert!(!sources.is_empty(), "no source file was read");
    let found = undeclared(&sources, &concepts);
    assert!(found.homes.is_empty(), "homes in docs/concepts.md that name no type: {:?}", found.homes);
}
