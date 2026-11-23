# Protocol

How a caller runs `sql-lineage`, and what it answers. Protocol 1 gives each write as a
query and its target, as dbt-lsp-plus gives a compiled model, or as a statement and its
target, and answers each write with its output columns, edges, gaps or refusal, in the
terms of `docs/concepts.md`.

## Uses

| Use | Status | Standard output | Standard error |
|---|---|---|---|
| `sql-lineage`, a request on standard input | 0 | the response, as JSON | nothing |
| the same, a request refused as a whole | 2 | nothing | one line, naming the reason |
| `sql-lineage --describe` | 0 | the description, as JSON | nothing |
| any other argument | 2 | nothing | one line, naming what was given |

## The request

One JSON object, of which every object refuses a field that it does not name:

```json
{
  "protocol": 1,
  "dialect": "snowflake",
  "defaults": {"database": "DB", "schema": "STAGING"},
  "catalog": [{"relation": "DB.RAW.ORDERS", "columns": ["ID", "AMOUNT_CENTS"]}],
  "writes": [
    {"id": "model.shop.stg_orders", "target": "STG_ORDERS",
     "sql": "select id as order_id, amount_cents / 100 as amount from raw.orders where amount_cents > 0"},
    {"id": "model.shop.orders", "target": "ORDERS",
     "sql": "select o.order_id, p.method from stg_orders o join payments p on p.order_id = o.order_id"}
  ]
}
```

- `protocol`: 1, read before any other field. Required.
- `dialect`: one of this build's, as `--describe` spells them. Required.
- `defaults`: optional; a `database` and a `schema`, each optional and one
  identifier, but a database needs a schema. Every name of a relation, in the catalog,
  a target or a write's SQL, that has fewer than three parts is completed with them: a
  missing schema, then a missing database.
- `catalog`: optional; the relations known before the first write, each a `relation`,
  a bare name of one to three parts, and its `columns`, at least one, each one
  identifier.
- `writes`: required, in dependency order; each an `id`, which its answer echoes; a
  `target`, a bare name, which may be in the catalog; and its `sql`, one query whose
  rows the target becomes, or one statement that writes the target: a replace
  statement, `CREATE VIEW`, `ALTER VIEW ... AS`, `CREATE TABLE ... AS`, a materialised
  view or a dynamic table, or `SELECT ... INTO`, whose query's rows the target becomes;
  or a contribute statement, `INSERT ... SELECT`, `UPDATE`, `DELETE` or `MERGE`, which
  adds rows to the target, changes or removes them, and leaves its columns as they are.
  Two writes with one target are read in the request's order, each against the catalog
  that the writes before it leave.

Every name is SQL text, read under the dialect and normalised by its rules for quoting
and case: in Snowflake, `orders` and `"ORDERS"` name one relation, and `"Orders"`
another.

A request is refused as a whole, in this order, for more than 512 MiB; bytes that are
not UTF-8; text that is not JSON; a `protocol` missing, not an integer, or not one
that this build reads; a field unknown, missing or of the wrong type; a dialect that is
not this build's, `generic` included; a default that is not one identifier, or a
database without a schema; a name of a relation or a target that is not a bare name,
or has text around it, such as spaces or a comment; a column's name that is not one
identifier; a relation with no column; two relations of the catalog, or two columns of
one relation, that name the same; two writes with one `id`.

## The response

One answer per write, in the request's order, under the request's `protocol`. The
response to the request above:

```json
{
  "protocol": 1,
  "answers": [
    {
      "write": "model.shop.stg_orders",
      "columns": ["order_id", "amount"],
      "edges": [
        {"parent": {"relation": "DB.RAW.ORDERS", "column": "AMOUNT_CENTS"},
         "child": {"relation": "STG_ORDERS", "column": "order_id"}, "kind": "rows"},
        {"parent": {"relation": "DB.RAW.ORDERS", "column": "ID"},
         "child": {"relation": "STG_ORDERS", "column": "order_id"}, "kind": "copy"},
        {"parent": {"relation": "DB.RAW.ORDERS", "column": "AMOUNT_CENTS"},
         "child": {"relation": "STG_ORDERS", "column": "amount"}, "kind": "derive"}
      ],
      "gaps": [],
      "refusal": null
    },
    {
      "write": "model.shop.orders",
      "columns": ["order_id", "method"],
      "edges": [
        {"parent": {"relation": "STG_ORDERS", "column": "order_id"},
         "child": {"relation": "ORDERS", "column": "order_id"}, "kind": "copy"},
        {"parent": {"relation": "STG_ORDERS", "column": "order_id"},
         "child": {"relation": "ORDERS", "column": "method"}, "kind": "rows"}
      ],
      "gaps": [
        {"reason": "unknown_relation", "relation": "payments", "column": null,
         "detail": "unknown to the catalog"}
      ],
      "refusal": null
    }
  ]
}
```

- `write`: the write's `id`.
- `columns`: the write's output columns, in order, each named as its query, or its
  statement's column list, names it, or `null` when they are not all known or the write
  is refused; for a contribute statement, the target's columns that it writes, in the
  target's order, or `null` when the catalog does not know the target.
- `edges`: each a `parent` and a `child`, a `relation` and a `column` spelled as the
  catalog or the write that made them known spells them, the child's relation as the
  write's `target`; and a `kind`, `copy`, `derive` or `rows`. One edge per pair of
  columns, ordered by the child column's place, then by the parent's relation and
  column.
- `gaps`: where an edge may be missing, each a `reason`: `wildcard`,
  `unknown_relation`, `unresolved_reference`, `ambiguous_relation`,
  `unattributed_use` or `analysis_failed`; the `relation` it concerns, as the write's
  SQL spells it, or `null`; the output `column` it concerns, or `null`; and a `detail`,
  text for a person, which this contract does not fix. Ordered by these four fields.
- `refusal`: `null`, or a `reason` and a `detail`. The reasons: `unparsed`, not one
  statement under the dialect; `not_a_query`, neither a query nor a statement that this
  build reads as a write, such as `INSERT ... VALUES`; `other_target`, a statement
  whose own target is not the write's `target`; `invalid`, not valid against the
  catalog, such as a column that the target lacks, or a column list of another length
  than the query's output columns; `unnamed_column`, an output column that its query
  does not name; `duplicate_column`, two output columns named alike. A refused answer
  has no edge and no gap.

After each write, the catalog changes for the writes after it: a query or a replace
statement that is refused, a write that does not parse, or one whose output columns are
not all known, makes its target unknown; any other query or replace statement gives its
target its output columns; a contribute statement, refused or not, and a statement
refused as `not_a_query` or `other_target`, leave the catalog as it was.

Within protocol 1, a response may gain a field, which a caller ignores, or a reason of
a gap or a refusal, which a caller reads as one of no known reason; and a write once
refused may come to be read. The request's fields and their meanings do not change.

## `--describe`

What this build is:

```json
{
  "tool": "sql-lineage",
  "version": "0.0.0",
  "protocols": [1],
  "engine": {
    "name": "polyglot-sql",
    "version": "0.14.3",
    "source": "registry+https://github.com/rust-lang/crates.io-index"
  },
  "dialects": ["snowflake", "tsql", "duckdb"]
}
```

- `version`: the version of `sql-lineage`.
- `protocols`: the versions of the request that this build reads.
- `engine`: the SQL engine, with its version and source as `Cargo.lock` pins them. The
  source is given verbatim, so a fork would show its commit.
- `dialects`: the dialects of this build, in this order; a request may name no other.
  A build may leave some out, by its Cargo features.
