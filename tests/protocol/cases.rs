// SPDX-License-Identifier: EUPL-1.2
//! The proof of concept's Snowflake cases, from its `cases/snowflake.py`, each one write
//! whose target is `T`, against the file's relations. Their expectations are the file's,
//! which takes them from its `definition.md`, in protocol 1's terms: an edge spells its
//! parent as the catalog does, and its child as the write's query, or a `*`, does; an
//! "error" is a refusal and a "gap" a gap and no edge; its one case "outside", a `MERGE`,
//! is read as plan 006 reads a `MERGE`. Five wait on polyglot: their tests are ignored,
//! each naming the issue that it waits on; and the pivot's `FOR` column is a `derive`, as
//! the Edge entry calls `CASE`.

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
    qualify: "select id, customer_id from db.s.orders \
        qualify row_number() over (partition by customer_id order by created_at desc) = 1" => [
        "ORDERS.ID>id copy",
        "ORDERS.CUSTOMER_ID>customer_id copy",
        "ORDERS.CUSTOMER_ID>id rows",
        "ORDERS.CREATED_AT>id customer_id rows",
    ];
    lateral_flatten: "select o.id, f.value:sku::string as sku from db.s.orders o, lateral flatten(input => o.items) f" =>
        ["ORDERS.ID>id copy", "ORDERS.ITEMS>sku derive", "ORDERS.ITEMS>id rows"];
    semi_structured_paths: "select payload:customer.name::string as customer_name, \
        payload:\"Total\"::number(10, 2) as total, payload['tags'][0] as first_tag from db.s.events" =>
        ["EVENTS.PAYLOAD>customer_name total first_tag derive"];
    select_exclude: "select * exclude (secret) from db.s.users" => ["USERS.ID>ID copy", "USERS.NAME>NAME copy"];
    select_rename: "select * rename (name as full_name) from db.s.users" =>
        ["USERS.ID>ID copy", "USERS.NAME>full_name copy", "USERS.SECRET>SECRET copy"];
    select_replace: "select * replace (upper(name) as name) from db.s.users" =>
        ["USERS.ID>ID copy", "USERS.NAME>NAME derive", "USERS.SECRET>SECRET copy"];
    qualified_star_exclude_over_a_join: "select o.* exclude (items, created_at), c.name from db.s.orders o \
        join db.s.customers c on o.customer_id = c.customer_id" => [
        "ORDERS.ID>ID copy",
        "ORDERS.CUSTOMER_ID>CUSTOMER_ID copy",
        "ORDERS.AMOUNT>AMOUNT copy",
        "ORDERS.STATUS>STATUS copy",
        "CUSTOMERS.NAME>name copy",
        "ORDERS.CUSTOMER_ID>ID AMOUNT STATUS name rows",
        "CUSTOMERS.CUSTOMER_ID>ID CUSTOMER_ID AMOUNT STATUS name rows",
    ];
    identifier: "select id from identifier('db.s.users')" => ["USERS.ID>id copy"];
    #[ignore = "known failure: polyglot #559"]
    pivot: "select * from (select region, quarter, amount from db.s.sales) \
        pivot (sum(amount) for quarter in ('Q1', 'Q2')) as p" => [
        "SALES.REGION>region copy",
        "SALES.AMOUNT SALES.QUARTER>\"'Q1'\" \"'Q2'\" derive",
        "SALES.REGION>\"'Q1'\" \"'Q2'\" rows",
    ];
    #[ignore = "known failure: polyglot #560"]
    unpivot: "select id, metric, val from db.s.wide unpivot (val for metric in (a, b))" =>
        ["WIDE.ID>id copy", "WIDE.A WIDE.B>val copy", "WIDE.A WIDE.B>id metric rows"];
    asof_join: "select t.id, t.ts, q.price from db.s.trades t asof join db.s.quotes q \
        match_condition (t.ts >= q.ts) on t.sym = q.sym" => [
        "TRADES.ID>id copy",
        "TRADES.TS>ts copy",
        "QUOTES.PRICE>price copy",
        "TRADES.TS>id price rows",
        "QUOTES.TS TRADES.SYM QUOTES.SYM>id ts price rows",
    ];
    window_functions: "select id, sum(amount) over (partition by customer_id order by created_at \
        rows between unbounded preceding and current row) as running, \
        lag(amount) over (partition by customer_id order by created_at) as prev from db.s.orders" =>
        ["ORDERS.ID>id copy", "ORDERS.AMOUNT ORDERS.CUSTOMER_ID ORDERS.CREATED_AT>running prev derive"];
    using_join_aliases_qualified_star: "select c.*, o.id as order_id \
        from db.s.customers c left join db.s.orders o using (customer_id)" => [
        "CUSTOMERS.CUSTOMER_ID>CUSTOMER_ID copy",
        "CUSTOMERS.NAME>NAME copy",
        "CUSTOMERS.EMAIL>EMAIL copy",
        "ORDERS.ID>order_id copy",
        "CUSTOMERS.CUSTOMER_ID>NAME EMAIL order_id rows",
        "ORDERS.CUSTOMER_ID>CUSTOMER_ID NAME EMAIL order_id rows",
    ];
    union_all_by_name: "select id, name from db.s.a union all by name select name, id from db.s.b" =>
        ["A.ID B.ID>id copy", "A.NAME B.NAME>name copy"];
    quoted_identifiers: "select id, \"Amount\" as amt from db.s.\"MixedCase\"" =>
        ["MIXED.ID>id copy", "MIXED.\"Amount\">amt copy"];
    cte_chain: "with a as (select id, amount from db.s.orders), \
        b as (select id, amount * 2 as double_amount from a where amount > 0), c as (select * from b) \
        select id, double_amount from c" =>
        ["ORDERS.ID>id copy", "ORDERS.AMOUNT>double_amount derive", "ORDERS.AMOUNT>id rows"];
    incremental_filter_on_this: "select id, payload, updated_at from db.s.events where updated_at > \
        (select coalesce(max(updated_at), '1900-01-01'::timestamp) from db.s.my_model)" => [
        "EVENTS.ID>id copy",
        "EVENTS.PAYLOAD>payload copy",
        "EVENTS.UPDATED_AT>updated_at copy",
        "EVENTS.UPDATED_AT>id payload rows",
        "MY_MODEL.UPDATED_AT>id payload updated_at rows",
    ];
    group_by_all: "select customer_id, sum(amount) as total from db.s.orders group by all" =>
        ["ORDERS.CUSTOMER_ID>customer_id copy", "ORDERS.AMOUNT>total derive", "ORDERS.CUSTOMER_ID>total rows"];
    lateral_column_alias: "select amount * 2 as doubled, doubled + 1 as plus from db.s.orders" =>
        ["ORDERS.AMOUNT>doubled plus derive"];
    time_travel: "select id from db.s.orders at(offset => -60)" => ["ORDERS.ID>id copy"];
    one_schema_and_table_in_two_databases: "select x.a, y.b from db1.s.t x join db2.s.t y on x.a = y.b" =>
        ["T1.A>a copy", "T2.B>b copy", "T1.A>b rows", "T2.B>a rows"];
    one_table_name_in_two_schemas: "select a.amount, r.amount_cents \
        from db.analytics.orders a join db.raw.orders r using (id)" =>
        ["MART.AMOUNT>amount copy", "RAW.AMOUNT_CENTS>amount_cents copy", "MART.ID RAW.ID>amount amount_cents rows"];
    #[ignore = "known failure: polyglot #557"]
    table_flatten: "select o.id, f.value:sku::string as sku from db.s.orders o, table(flatten(input => o.items)) f" =>
        ["ORDERS.ID>id copy", "ORDERS.ITEMS>sku derive", "ORDERS.ITEMS>id rows"];
    #[ignore = "known failure: polyglot #561"]
    select_ilike: "select * ilike '%name%' from db.s.users" => ["USERS.NAME>NAME copy"];
    #[ignore = "known failure: polyglot #556"]
    object_construct: "select id, object_construct(*) as obj from db.s.users" =>
        ["USERS.ID>id copy", "USERS.ID USERS.NAME USERS.SECRET>obj derive"];
}

#[test]
fn unquoted_column_against_a_quoted_one() {
    assert_eq!(case("select amount from db.s.\"MixedCase\""), ["refused invalid"]);
}

#[test]
fn unquoted_relation_against_a_quoted_one() {
    assert_eq!(case("select id from db.s.mixedcase"), ["gap unknown_relation db.s.mixedcase -"]);
}

/// The file's `merge`, outside its scope then, written to its own target: its `ON`
/// decides the rows of every column of the target from the source's key, and from none of
/// the target's own (Q4); each `WHEN` copies the columns that it sets.
#[test]
fn merge() {
    let sql = "merge into db.s.my_model t using db.s.events s on t.id = s.id \
        when matched then update set t.updated_at = s.updated_at \
        when not matched then insert (id, updated_at) values (s.id, s.updated_at)";
    let catalog: Vec<(&str, &[&str])> = RELATIONS.iter().map(|(_, relation, columns)| (*relation, *columns)).collect();
    let expected = [
        "DB.S.EVENTS.ID -> DB.S.MY_MODEL.ID copy",
        "DB.S.EVENTS.ID -> DB.S.MY_MODEL.PAYLOAD rows",
        "DB.S.EVENTS.ID -> DB.S.MY_MODEL.UPDATED_AT rows",
        "DB.S.EVENTS.UPDATED_AT -> DB.S.MY_MODEL.UPDATED_AT copy",
    ];
    assert_eq!(outline(&respond(&request(&catalog, &[("DB.S.MY_MODEL", sql)]))[0]), expected);
}
