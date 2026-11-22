//! The concept check: every type in `src/` has a home in `docs/concepts.md`, every home
//! names a type, and no type's name is defined twice.  A type is a struct, an enum, a
//! union, a trait or a type alias outside `#[cfg(test)]`, wherever it is defined and
//! whatever its visibility; a home is a name in backticks on a `home:` line, or in the
//! Home cell of a table's row.

use std::collections::BTreeSet;
use std::path::Path;

use syn::visit::Visit;

/// What the check finds wrong.
#[derive(Debug, Default, PartialEq)]
struct Undeclared {
    /// The types that no home names.
    types: BTreeSet<String>,
    /// The names in a home that are no type.
    homes: BTreeSet<String>,
    /// The names of types defined more than once, which one home cannot tell apart.
    duplicates: BTreeSet<String>,
}

/// The types that a walk of the sources' syntax trees finds.
#[derive(Default)]
struct Types {
    names: BTreeSet<String>,
    duplicates: BTreeSet<String>,
}

impl Types {
    fn record(&mut self, attrs: &[syn::Attribute], name: &syn::Ident) {
        if !test_only(attrs) && !self.names.insert(name.to_string()) {
            self.duplicates.insert(name.to_string());
        }
    }
}

impl<'ast> Visit<'ast> for Types {
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.record(&item.attrs, &item.ident);
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.record(&item.attrs, &item.ident);
    }

    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        self.record(&item.attrs, &item.ident);
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.record(&item.attrs, &item.ident);
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        self.record(&item.attrs, &item.ident);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !test_only(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) {
            syn::visit::visit_item_fn(self, item);
        }
    }
}

/// Whether `attrs` hold `#[cfg(test)]`.
fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("cfg") && attr.parse_args::<syn::Ident>().is_ok_and(|arg| arg == "test"))
}

/// The types of `sources` that no home of `concepts` names, the homes that name no type,
/// and the names defined twice.
fn undeclared(sources: &[String], concepts: &str) -> Undeclared {
    let mut types = Types::default();
    for source in sources {
        types.visit_file(&syn::parse_file(source).expect("a source file parses"));
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
        types: types.names.difference(&homes).cloned().collect(),
        homes: homes.difference(&types.names).cloned().collect(),
        duplicates: types.duplicates,
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
    let source = "struct Answer; struct Request; enum Stray {} union Loose { a: u8 }";
    let found = undeclared(&[source.to_string()], CONCEPTS);
    assert_eq!(found, Undeclared { types: names(&["Loose", "Stray"]), ..Undeclared::default() });
}

#[test]
fn concepts_report_a_home_with_no_type() {
    let found = undeclared(&["struct Answer;".to_string()], CONCEPTS);
    assert_eq!(found, Undeclared { homes: names(&["Request"]), ..Undeclared::default() });
}

#[test]
fn concepts_report_a_name_defined_twice() {
    let sources = ["struct Answer; struct Request;".to_string(), "mod other { pub enum Request {} }".to_string()];
    let found = undeclared(&sources, CONCEPTS);
    assert_eq!(found, Undeclared { duplicates: names(&["Request"]), ..Undeclared::default() });
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
        #[cfg(test)] mod tests { struct Fixture; }
        #[cfg(test)] fn fixture() { struct Local; }";
    assert_eq!(undeclared(&[source.to_string()], CONCEPTS), Undeclared::default());
}

#[test]
fn concepts_find_types_in_inline_modules() {
    let source = "struct Answer; mod inner { pub struct Request; type Stray = u8; }";
    let found = undeclared(&[source.to_string()], CONCEPTS);
    assert_eq!(found, Undeclared { types: names(&["Stray"]), ..Undeclared::default() });
}

#[test]
fn concepts_find_types_in_function_bodies() {
    let source = "struct Answer; struct Request; fn main() { struct Hidden; }";
    let found = undeclared(&[source.to_string()], CONCEPTS);
    assert_eq!(found, Undeclared { types: names(&["Hidden"]), ..Undeclared::default() });
}

#[test]
fn every_type_has_a_home() {
    let (sources, concepts) = repository();
    assert!(!sources.is_empty(), "no source file was read");
    let found = undeclared(&sources, &concepts);
    assert!(found.types.is_empty(), "types with no home in docs/concepts.md: {:?}", found.types);
    assert!(found.duplicates.is_empty(), "type names defined twice: {:?}", found.duplicates);
}

#[test]
fn every_home_names_a_type() {
    let (sources, concepts) = repository();
    assert!(!sources.is_empty(), "no source file was read");
    let found = undeclared(&sources, &concepts);
    assert!(found.homes.is_empty(), "homes in docs/concepts.md that name no type: {:?}", found.homes);
}
