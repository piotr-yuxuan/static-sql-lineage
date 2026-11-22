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

/// What a walk of the sources' syntax trees finds defined.
#[derive(Default)]
struct Definitions {
    names: BTreeSet<String>,
    duplicates: BTreeSet<String>,
}

impl Definitions {
    fn record(&mut self, attrs: &[syn::Attribute], name: &syn::Ident) {
        if !test_only(attrs) && !self.names.insert(name.to_string()) {
            self.duplicates.insert(name.to_string());
        }
    }
}

impl<'ast> Visit<'ast> for Definitions {
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

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if !test_only(&item.attrs) {
            syn::visit::visit_item_impl(self, item);
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
    let mut types = Definitions::default();
    for source in sources {
        types.visit_file(&syn::parse_file(source).expect("a source file parses"));
    }
    let homes: BTreeSet<String> = (read_concepts(concepts).fillings.into_iter())
        .filter_map(|(_, filling)| match filling {
            Filling::Home(name) => Some(name),
            Filling::Item(_) => None,
        })
        .collect();
    Undeclared {
        types: types.names.difference(&homes).cloned().collect(),
        homes: homes.difference(&types.names).cloned().collect(),
        duplicates: types.duplicates,
    }
}

/// A filling of `docs/concepts.md`: the name of a home, or an item of an operations
/// list, by its whole text.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Filling {
    Home(String),
    Item(String),
}

/// `docs/concepts.md` read apart: its skeleton, the file line for line with its fillings
/// set aside, and each filling with its entry.
#[derive(Default)]
struct Concepts {
    skeleton: Vec<String>,
    fillings: BTreeSet<(String, Filling)>,
}

impl Concepts {
    /// Records `fillings` under `entry` and keeps `blank`, if any, in the skeleton; or,
    /// when `fillings` is none because `line` is not in its shape, keeps `line` itself.
    fn fill(&mut self, entry: &str, fillings: Option<Vec<Filling>>, line: &str, blank: Option<String>) {
        match fillings {
            Some(fillings) => {
                self.fillings.extend(fillings.into_iter().map(|filling| (entry.to_string(), filling)));
                self.skeleton.extend(blank);
            }
            None => self.skeleton.push(line.to_string()),
        }
    }
}

/// `concepts` read apart.  A home line is `- home: none yet.` or names in backticks
/// separated by `, `, then `.`; a Home cell is empty or such names; an operations line is
/// `- operations: none yet.`, or `- operations:` followed by items, each one line
/// `` - `name`: what it does ``, indented by two spaces.  An entry is the nearest `## `
/// heading, and for a table row the heading and the row's first cell.  A line that would
/// be a filling but is not in its shape stays in the skeleton.
fn read_concepts(concepts: &str) -> Concepts {
    let mut read = Concepts::default();
    let (mut entry, mut home_column, mut listing) = (String::new(), None, false);
    for line in concepts.lines() {
        let cells: Vec<&str> =
            line.strip_prefix('|').map(|row| row.split('|').map(str::trim).collect()).unwrap_or_default();
        let item = line.strip_prefix("  - ").filter(|_| listing);
        listing = item.is_some() || line == "- operations:";
        entry = line.strip_prefix("## ").map_or(entry, str::to_string);
        home_column = cells.iter().position(|cell| *cell == "Home").or(home_column.filter(|_| !cells.is_empty()));
        let home_cell = (home_column.and_then(|column| cells.get(column)))
            .filter(|cell| **cell != "Home" && !line.starts_with("|-"));
        if let Some(home) = line.strip_prefix("- home: ") {
            read.fill(&entry, home.strip_suffix('.').and_then(names_in), line, Some("- home:".to_string()));
        } else if let Some(rest) = line.strip_prefix("- operations:") {
            read.fill(&entry, ["", " none yet."].contains(&rest).then(Vec::new), line, Some("- operations:".into()));
        } else if let Some(text) = item {
            let named = text.strip_prefix('`').and_then(|rest| rest.split_once("`: "));
            let shaped = named.filter(|(name, what)| name.split("::").all(is_identifier) && !what.is_empty());
            read.fill(&entry, shaped.map(|_| vec![Filling::Item(line.to_string())]), line, None);
        } else if let Some(cell) = home_cell {
            let blank: Vec<&str> =
                cells.iter().enumerate().map(|(i, other)| if Some(i) == home_column { "" } else { other }).collect();
            read.fill(&format!("{entry} / {}", cells[0]), names_in(cell), line, Some(blank.join(" | ")));
        } else {
            read.skeleton.push(line.to_string());
        }
    }
    read
}

/// The names of a home: none for an empty text or `none yet`, else names in backticks
/// separated by `, `; none at all when `text` is in no such shape.
fn names_in(text: &str) -> Option<Vec<Filling>> {
    if text.is_empty() || text == "none yet" {
        return Some(Vec::new());
    }
    (text.split(", "))
        .map(|part| part.strip_prefix('`')?.strip_suffix('`').filter(|name| is_identifier(name)))
        .map(|name| name.map(|name| Filling::Home(name.to_string())))
        .collect()
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// What `head` changes in `docs/concepts.md` beyond fillings, against `base`: the first
/// line of the skeletons that differs, then each filling of `base` that `head` lost from
/// its entry.
fn kernel_changes(base: &str, head: &str) -> Vec<String> {
    let (base, head) = (read_concepts(base), read_concepts(head));
    let mut changes = Vec::new();
    if base.skeleton != head.skeleton {
        let at = (base.skeleton.iter().zip(&head.skeleton).position(|(old, new)| old != new))
            .unwrap_or(base.skeleton.len().min(head.skeleton.len()));
        changes.push(format!("{:?} became {:?}", base.skeleton.get(at), head.skeleton.get(at)));
    }
    for (entry, filling) in base.fillings.difference(&head.fillings) {
        changes.push(match filling {
            Filling::Home(name) => format!("{entry} lost the home `{name}`"),
            Filling::Item(item) => format!("{entry} lost the item {item:?}"),
        });
    }
    changes
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
fn concepts_ignore_test_impls() {
    let source = "struct Answer; struct Request;
        #[cfg(test)] impl Answer { fn fixture() { struct Local; } }";
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

/// A file of concepts for the tests of `kernel_changes`.
const KERNEL: &str = "\
## Answer

- home: none yet.
- operations: none yet.

## Request

- home: `Request`.
- operations:
  - `Request::read`: reads one request.

| Term | Defined as | Home |
|---|---|---|
| request | one JSON document | |
| protocol | its number | |
";

#[test]
fn kernel_allows_a_first_fill() {
    let head = (KERNEL.replace("- home: none yet.", "- home: `Answer`."))
        .replace("- operations: none yet.", "- operations:\n  - `Answer::of`: builds one.")
        .replace("| request | one JSON document | |", "| request | one JSON document | `Request` |");
    assert_eq!(kernel_changes(KERNEL, &head), Vec::<String>::new());
}

#[test]
fn kernel_allows_an_added_item() {
    let head = KERNEL.replace("reads one request.\n", "reads one request.\n  - `Request::write`: writes one.\n");
    assert_eq!(kernel_changes(KERNEL, &head), Vec::<String>::new());
}

#[test]
fn kernel_refuses_a_removed_name() {
    let changes = kernel_changes(KERNEL, &KERNEL.replace("- home: `Request`.", "- home: none yet."));
    assert!(matches!(changes.as_slice(), [change] if change.contains("Request") && change.contains("`Request`")));
}

#[test]
fn kernel_refuses_a_name_moved_to_another_entry() {
    let base = KERNEL.replace("| request | one JSON document | |", "| request | one JSON document | `Request` |");
    let head = KERNEL.replace("| protocol | its number | |", "| protocol | its number | `Request` |");
    let changes = kernel_changes(&base, &head);
    assert!(matches!(changes.as_slice(), [change] if change.contains("Request / request")), "{changes:?}");
}

#[test]
fn kernel_refuses_a_reworded_item() {
    let changes = kernel_changes(KERNEL, &KERNEL.replace("reads one request.", "reads a request."));
    assert!(matches!(changes.as_slice(), [change] if change.contains("reads one request")), "{changes:?}");
}

#[test]
fn kernel_refuses_a_changed_line() {
    let changes = kernel_changes(KERNEL, &KERNEL.replace("| its number |", "| its version |"));
    assert!(matches!(changes.as_slice(), [change] if change.contains("its version")), "{changes:?}");
}

#[test]
fn kernel_refuses_prose_in_a_home() {
    let head = KERNEL.replace("- home: `Request`.", "- home: `Request`; and a CTE is also a relation.");
    let changes = kernel_changes(KERNEL, &head);
    assert!(changes.iter().any(|change| change.contains("a CTE is also a relation")), "{changes:?}");
}

#[test]
fn kernel_refuses_prose_in_an_item() {
    let head = KERNEL.replace("reads one request.\n", "reads one request.\n    And a write may have no target.\n");
    let changes = kernel_changes(KERNEL, &head);
    assert!(changes.iter().any(|change| change.contains("a write may have no target")), "{changes:?}");
}

/// `docs/concepts.md` where the branch began against `HEAD`, as `make kernel-unchanged`
/// gives them.
#[test]
#[ignore = "make kernel-unchanged runs it, with the two files"]
fn kernel_unchanged() {
    let read = |variable: &str| {
        let path = std::env::var(variable).unwrap_or_else(|_| panic!("{variable} is unset: run make kernel-unchanged"));
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    let changes = kernel_changes(&read("SQL_LINEAGE_CONCEPTS_BASE"), &read("SQL_LINEAGE_CONCEPTS_HEAD"));
    assert!(changes.is_empty(), "docs/concepts.md changed beyond fillings:\n{}", changes.join("\n"));
}
