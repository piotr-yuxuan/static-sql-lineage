// SPDX-License-Identifier: EUPL-1.2
//! The proof of concept's Snowflake cases, from its `cases/snowflake.py`, each one write
//! whose target is `T`, against the file's relations. Their expectations are the file's,
//! which takes them from its `definition.md`, in protocol 1's terms: an edge spells its
//! parent as the catalog does, and its child as the write's query, or a `*`, does.

use super::{outline, request, respond};

/// The file's relations, each by its short name, with its columns. As R7 of
/// `docs/decisions.md` rules, `"MixedCase"` declares `"Amount"`, which the file declares
/// bare to mean its exact name.
const RELATIONS: &[(&str, &str, &[&str])] = &[
    ("ORDERS", "DB.S.ORDERS", &["ID", "CUSTOMER_ID", "AMOUNT", "STATUS", "CREATED_AT", "ITEMS"]),
    ("CUSTOMERS", "DB.S.CUSTOMERS", &["CUSTOMER_ID", "NAME", "EMAIL"]),
    ("USERS", "DB.S.USERS", &["ID", "NAME", "SECRET"]),
    ("EVENTS", "DB.S.EVENTS", &["ID", "PAYLOAD", "UPDATED_AT"]),
    ("SALES", "DB.S.SALES", &["REGION", "QUARTER", "AMOUNT"]),
    ("WIDE", "DB.S.WIDE", &["ID", "A", "B"]),
    ("TRADES", "DB.S.TRADES", &["ID", "SYM", "TS"]),
    ("QUOTES", "DB.S.QUOTES", &["SYM", "TS", "PRICE"]),
    ("A", "DB.S.A", &["ID", "NAME"]),
    ("B", "DB.S.B", &["NAME", "ID"]),
    ("MIXED", "DB.S.\"MixedCase\"", &["ID", "\"Amount\""]),
    ("MY_MODEL", "DB.S.MY_MODEL", &["ID", "PAYLOAD", "UPDATED_AT"]),
    ("T1", "DB1.S.T", &["A"]),
    ("T2", "DB2.S.T", &["B"]),
    ("RAW", "DB.RAW.ORDERS", &["ID", "AMOUNT_CENTS"]),
    ("MART", "DB.ANALYTICS.ORDERS", &["ID", "AMOUNT"]),
];

/// The answer to `sql` in short, sorted.
fn case(sql: &str) -> Vec<String> {
    let catalog: Vec<(&str, &[&str])> = RELATIONS.iter().map(|(_, relation, columns)| (*relation, *columns)).collect();
    let mut found = outline(&respond(&request(&catalog, &[("T", sql)]))[0]);
    found.sort();
    found
}

/// The edges that `groups` give, sorted, each group as the file writes `rows()`:
/// `PARENTS>CHILDREN kind`, every parent, `SHORT.COLUMN`, to every child.
fn edges(groups: &[&str]) -> Vec<String> {
    let mut edges: Vec<String> = (groups.iter())
        .flat_map(|group| {
            let (ends, kind) = group.rsplit_once(' ').expect("a kind");
            let (parents, children) = ends.split_once('>').expect("parents and children");
            parents.split(' ').flat_map(move |parent| {
                let (short, column) = parent.split_once('.').expect("a short name and a column");
                let (_, relation, _) = RELATIONS.iter().find(|(name, ..)| *name == short).expect("a relation");
                children.split(' ').map(move |child| format!("{relation}.{column} -> T.{child} {kind}"))
            })
        })
        .collect();
    edges.sort();
    edges
}

/// One test per case of the file, named after it, with the edges that it expects.
macro_rules! cases {
    ($($(#[$attribute:meta])* $name:ident: $sql:expr => [$($group:expr),* $(,)?];)*) => {
        $(
            #[test]
            $(#[$attribute])*
            fn $name() {
                assert_eq!(case($sql), edges(&[$($group),*]));
            }
        )*
    };
}

cases! {
    identifier: "select id from identifier('db.s.users')" => ["USERS.ID>id copy"];
}
