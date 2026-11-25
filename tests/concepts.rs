// SPDX-License-Identifier: EUPL-1.2
//! The concept check: every type in `src/` has a home in `docs/concepts.md`, every home
//! names a type, no type's name is defined twice, every operation names a function, and
//! every filling is in its shape.  A type is a struct, an enum, a union, a trait or a
//! type alias outside tests, wherever it is defined and whatever its visibility; a home
//! is a name in backticks on a `home:` line, or in the Home cell of a table's row.  Also
//! `kernel_changes`, which `make kernel-unchanged` runs.

#[macro_use]
#[path = "protocol/rows.rs"]
mod rows;

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
    /// The functions that an operations item names and that do not exist, or are not
    /// visible outside their module, or serve tests only.
    operations: BTreeSet<String>,
    /// The lines that would fill a home or an operations list but are not in its shape.
    malformed: BTreeSet<String>,
}

/// What a walk of the sources' syntax trees finds defined.
#[derive(Default)]
struct Definitions {
    names: BTreeSet<String>,
    duplicates: BTreeSet<String>,
    /// The functions that an operations item may name: `name` for a free function,
    /// `Type::name` for one of `Type`'s, each visible outside its module.
    functions: BTreeSet<String>,
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
        let visible = !matches!(item.vis, syn::Visibility::Inherited) && !test_only(&item.attrs);
        let methods = item.items.iter().filter_map(|member| match member {
            syn::TraitItem::Fn(method) if visible && !test_only(&method.attrs) => Some(&method.sig.ident),
            _ => None,
        });
        self.functions.extend(methods.map(|method| format!("{}::{method}", item.ident)));
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
            if !matches!(item.vis, syn::Visibility::Inherited) {
                self.functions.insert(item.sig.ident.to_string());
            }
            syn::visit::visit_item_fn(self, item);
        }
    }

    /// A method of a trait's `impl` is visible wherever the trait is; one of an inherent
    /// `impl` must say so.
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if test_only(&item.attrs) {
            return;
        }
        if let syn::Type::Path(owner) = &*item.self_ty
            && let Some(owner) = owner.path.segments.last()
        {
            let methods = item.items.iter().filter_map(|member| match member {
                syn::ImplItem::Fn(method)
                    if !test_only(&method.attrs)
                        && (item.trait_.is_some() || !matches!(method.vis, syn::Visibility::Inherited)) =>
                {
                    Some(&method.sig.ident)
                }
                _ => None,
            });
            self.functions.extend(methods.map(|method| format!("{}::{method}", owner.ident)));
        }
        syn::visit::visit_item_impl(self, item);
    }

    fn visit_impl_item_fn(&mut self, method: &'ast syn::ImplItemFn) {
        if !test_only(&method.attrs) {
            syn::visit::visit_impl_item_fn(self, method);
        }
    }
}

/// Whether `attrs` hold `#[cfg(test)]` or `#[test]`.
fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || attr.path().is_ident("cfg") && attr.parse_args::<syn::Ident>().is_ok_and(|arg| arg == "test")
    })
}

/// The types of `sources` that no home of `concepts` names, the homes that name no type,
/// the names defined twice, the operations that name no function, and the fillings out
/// of their shape.
fn undeclared(sources: &[String], concepts: &str) -> Undeclared {
    let mut definitions = Definitions::default();
    for source in sources {
        definitions.visit_file(&syn::parse_file(source).expect("a source file parses"));
    }
    let read = read_concepts(concepts);
    let (mut homes, mut operations) = (BTreeSet::new(), BTreeSet::new());
    for (_, filling) in read.fillings {
        match filling {
            Filling::Home(name) => _ = homes.insert(name),
            Filling::Item(item) => operations.extend(operation_in(&item).map(str::to_string)),
        }
    }
    Undeclared {
        types: definitions.names.difference(&homes).cloned().collect(),
        homes: homes.difference(&definitions.names).cloned().collect(),
        duplicates: definitions.duplicates,
        operations: operations.difference(&definitions.functions).cloned().collect(),
        malformed: read.malformed.into_iter().collect(),
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
/// set aside, each filling with its entry, and the lines that would be fillings but are
/// not in their shape.
#[derive(Default)]
struct Concepts {
    skeleton: Vec<String>,
    fillings: BTreeSet<(String, Filling)>,
    malformed: Vec<String>,
}

impl Concepts {
    /// Records `fillings` under `entry` and keeps `blank`, if any, in the skeleton; or,
    /// when `fillings` is none because `line` is not in its shape, keeps `line` itself in
    /// the skeleton and records it as malformed.
    fn fill(&mut self, entry: &str, fillings: Option<Vec<Filling>>, line: &str, blank: Option<String>) {
        match fillings {
            Some(fillings) => {
                self.fillings.extend(fillings.into_iter().map(|filling| (entry.to_string(), filling)));
                self.skeleton.extend(blank);
            }
            None => {
                self.skeleton.push(line.to_string());
                self.malformed.push(line.to_string());
            }
        }
    }
}

/// `concepts` read apart.  A home line is `- home: none yet.` or names in backticks
/// separated by `, `, then `.`; a Home cell is empty or such names; an operations line is
/// `- operations: none yet.`, or `- operations:` followed by items, each one line
/// `` - `name`: what it does ``, indented by two spaces; a line indented by four among
/// them, an item wrapped, is in no shape.  An entry is the nearest `## ` heading, and for
/// a table row the heading and the row's first cell.  A line that would be a filling but
/// is not in its shape stays in the skeleton.
fn read_concepts(concepts: &str) -> Concepts {
    let mut read = Concepts::default();
    let (mut entry, mut home_column, mut listing) = (String::new(), None, false);
    for line in concepts.lines() {
        // A separator row, `|---|` or `| :-- |`, has no cells.
        let cells: Vec<&str> = (line.strip_prefix('|').filter(|row| !row.trim_start().starts_with(['-', ':'])))
            .map(|row| row.split('|').map(str::trim).collect())
            .unwrap_or_default();
        let item = listing && line.starts_with("  ");
        listing = item || line == "- operations:";
        entry = line.strip_prefix("## ").map_or(entry, str::to_string);
        // The column found in the header serves the rows after it, so the header itself
        // has no Home cell; it serves while the lines are the table's.
        let home_cell = home_column.and_then(|column| cells.get(column));
        home_column = cells.iter().position(|cell| *cell == "Home").or(home_column.filter(|_| line.starts_with('|')));
        if let Some(home) = line.strip_prefix("- home: ") {
            read.fill(&entry, home.strip_suffix('.').and_then(names_in), line, Some("- home:".to_string()));
        } else if let Some(rest) = line.strip_prefix("- operations:") {
            read.fill(&entry, ["", " none yet."].contains(&rest).then(Vec::new), line, Some("- operations:".into()));
        } else if item {
            read.fill(&entry, operation_in(line).map(|_| vec![Filling::Item(line.to_string())]), line, None);
        } else if let Some(cell) = home_cell {
            let others: Vec<&str> = (cells.iter().enumerate())
                .filter(|(column, _)| Some(*column) != home_column)
                .map(|(_, other)| *other)
                .collect();
            read.fill(&format!("{entry} / {}", cells[0]), names_in(cell), line, Some(others.join(" | ")));
        } else {
            read.skeleton.push(line.to_string());
        }
    }
    read
}

/// The function that an operations item names, when `line` is one in its shape:
/// `` - `name`: what it does ``, indented by two spaces, `name` an identifier or
/// `Type::name`.
fn operation_in(line: &str) -> Option<&str> {
    let (name, what) = line.strip_prefix("  - `")?.split_once("`: ")?;
    (name.split("::").all(is_identifier) && !what.is_empty()).then_some(name)
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

/// Asserts that the check finds `expected` in `source` against `CONCEPTS`.
#[track_caller]
fn finds(source: &str, expected: Undeclared) {
    assert_eq!(undeclared(&[source.to_string()], CONCEPTS), expected);
}

cases! {
    concepts_report_a_type_with_no_home: finds("struct Answer; struct Request; enum Stray {} union Loose { a: u8 }",
        Undeclared { types: names(&["Loose", "Stray"]), ..Undeclared::default() });
    concepts_report_a_home_with_no_type:
        finds("struct Answer;", Undeclared { homes: names(&["Request"]), ..Undeclared::default() });
    concepts_report_a_name_defined_twice: {
        let sources = ["struct Answer; struct Request;".to_string(), "mod other { pub enum Request {} }".to_string()];
        let found = undeclared(&sources, CONCEPTS);
        assert_eq!(found, Undeclared { duplicates: names(&["Request"]), ..Undeclared::default() });
    };
    concepts_ignore_associated_types: finds("struct Answer; struct Request;
        impl std::str::FromStr for Answer { type Err = (); fn from_str(_: &str) -> Result<Self, ()> { Err(()) } }",
        Undeclared::default());
    concepts_ignore_comments: finds("// struct Stray;
        /// Says struct Documented;
        struct Answer;
        struct Request;
        const TEXT: &str = \"struct Quoted;\";", Undeclared::default());
    concepts_ignore_test_types: finds("struct Answer; struct Request;
        #[cfg(test)] struct Helper;
        #[cfg(test)] mod tests { struct Fixture; }
        #[cfg(test)] fn fixture() { struct Local; }", Undeclared::default());
    concepts_ignore_test_impls: finds("struct Answer; struct Request;
        #[cfg(test)] impl Answer { fn fixture() { struct Local; } }
        impl Request { #[cfg(test)] fn fixture() { struct Inner; } }", Undeclared::default());
    concepts_find_types_in_inline_modules: finds("struct Answer; mod inner { pub struct Request; type Stray = u8; }",
        Undeclared { types: names(&["Stray"]), ..Undeclared::default() });
    concepts_find_types_in_function_bodies: finds("struct Answer; struct Request; fn main() { struct Hidden; }",
        Undeclared { types: names(&["Hidden"]), ..Undeclared::default() });
}

/// What the check finds wrong in the repository, of which a source file was read.
fn checked() -> Undeclared {
    let (sources, concepts) = repository();
    assert!(!sources.is_empty(), "no source file was read");
    undeclared(&sources, &concepts)
}

#[test]
fn every_type_has_a_home() {
    let found = checked();
    assert!(found.types.is_empty(), "types with no home in docs/concepts.md: {:?}", found.types);
    assert!(found.duplicates.is_empty(), "type names defined twice: {:?}", found.duplicates);
}

cases! {
    every_home_names_a_type: {
        let found = checked();
        assert!(found.homes.is_empty(), "homes in docs/concepts.md that name no type: {:?}", found.homes);
    };
    every_operation_names_a_function: {
        let found = checked();
        assert!(found.operations.is_empty(),
            "operations in docs/concepts.md that name no function: {:?}", found.operations);
    };
    every_filling_has_its_shape: {
        let found = checked();
        assert!(found.malformed.is_empty(), "lines of docs/concepts.md out of their shape: {:?}", found.malformed);
    };
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

/// What `kernel_changes` finds in `KERNEL` with `from` replaced by `to`.
fn changed(from: &str, to: &str) -> Vec<String> {
    kernel_changes(KERNEL, &KERNEL.replace(from, to))
}

/// Asserts that `changes` are one change, which names `naming`.
#[track_caller]
fn one_change(changes: &[String], naming: &str) {
    assert!(matches!(changes, [change] if change.contains(naming)), "{changes:?}");
}

cases! {
    kernel_allows_a_first_fill: {
        let head = (KERNEL.replace("- home: none yet.", "- home: `Answer`."))
            .replace("- operations: none yet.", "- operations:\n  - `Answer::of`: builds one.")
            .replace("| request | one JSON document | |", "| request | one JSON document | `Request` |");
        assert_eq!(kernel_changes(KERNEL, &head), Vec::<String>::new());
    };
    kernel_allows_an_added_item: assert_eq!(
        changed("reads one request.\n", "reads one request.\n  - `Request::write`: writes one.\n"),
        Vec::<String>::new()
    );
    kernel_refuses_a_removed_name: {
        let changes = changed("- home: `Request`.", "- home: none yet.");
        assert!(matches!(changes.as_slice(), [change] if change.contains("Request") && change.contains("`Request`")));
    };
    kernel_refuses_a_name_moved_to_another_entry: {
        let base = KERNEL.replace("| request | one JSON document | |", "| request | one JSON document | `Request` |");
        let head = KERNEL.replace("| protocol | its number | |", "| protocol | its number | `Request` |");
        one_change(&kernel_changes(&base, &head), "Request / request");
    };
    kernel_refuses_a_reworded_item: one_change(&changed("reads one request.", "reads a request."), "reads one request");
    kernel_refuses_a_changed_line: one_change(&changed("| its number |", "| its version |"), "its version");
    kernel_refuses_prose_in_a_home: {
        let changes = changed("- home: `Request`.", "- home: `Request`; and a CTE is also a relation.");
        assert!(changes.iter().any(|change| change.contains("a CTE is also a relation")), "{changes:?}");
    };
    kernel_refuses_prose_in_an_item: {
        let changes = changed("reads one request.\n", "reads one request.\n    And a write may have no target.\n");
        assert!(changes.iter().any(|change| change.contains("a write may have no target")), "{changes:?}");
    };
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

/// An entry that lists two operations: `Answer::of`, one of `Answer`'s, and `answer_all`,
/// a free function.
const OPERATIONS: &str = "\
## Answer

- home: `Answer`.
- operations:
  - `Answer::of`: builds one.
  - `answer_all`: answers every write.
";

fn operations_of(source: &str) -> BTreeSet<String> {
    undeclared(&[source.to_string()], OPERATIONS).operations
}

cases! {
    concepts_find_an_operation_in_an_impl: assert_eq!(
        operations_of("pub struct Answer; impl Answer { pub fn of() -> Self { Answer } }"),
        names(&["answer_all"])
    );
    concepts_find_a_free_function_operation:
        assert_eq!(operations_of("pub struct Answer; pub fn answer_all() {}"), names(&["Answer::of"]));
    concepts_report_an_operation_with_no_function:
        assert_eq!(operations_of("pub struct Answer;"), names(&["Answer::of", "answer_all"]));
    concepts_report_an_operation_of_another_type: assert_eq!(
        operations_of("pub struct Answer; pub struct Other; impl Other { pub fn of() {} } pub fn answer_all() {}"),
        names(&["Answer::of"])
    );
    concepts_report_a_method_named_as_a_free_function: assert_eq!(
        operations_of("pub struct Answer; impl Answer { pub fn of() {} pub fn answer_all() {} }"),
        names(&["answer_all"])
    );
    concepts_report_a_private_operation: assert_eq!(
        operations_of("pub struct Answer; impl Answer { fn of() {} } pub fn answer_all() {}"),
        names(&["Answer::of"])
    );
    concepts_report_a_test_only_operation: assert_eq!(
        operations_of("pub struct Answer; impl Answer { #[cfg(test)] pub fn of() {} } #[test] pub fn answer_all() {}"),
        names(&["Answer::of", "answer_all"])
    );
}

#[test]
fn concepts_report_a_malformed_filling() {
    let concepts = "\
## Answer

- home: `Answer`; and from now on a CTE is a relation.
- operations:
  - Answer::of builds one.
  - `answer_all`: answers every write.
    And it decides the dialect.
";
    let found = undeclared(&["pub struct Answer; pub fn answer_all() {}".to_string()], concepts);
    let malformed = names(&[
        "- home: `Answer`; and from now on a CTE is a relation.",
        "  - Answer::of builds one.",
        "    And it decides the dialect.",
    ]);
    assert_eq!(found.malformed, malformed);
}

#[test]
fn concepts_read_any_separator_row() {
    for separator in ["|---|---|---|", "| --- | --- | --- |", "|:--|:--|:--|"] {
        let concepts = format!("| Term | Defined as | Home |\n{separator}\n| answer | one per write | `Answer` |\n");
        let found = undeclared(&["struct Answer;".to_string()], &concepts);
        assert_eq!(found, Undeclared::default(), "with the separator {separator}");
    }
}
