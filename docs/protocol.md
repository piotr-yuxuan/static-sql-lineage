# Protocol

How a caller runs `static-sql-lineage`, and what it answers. Protocol 1 gives each
write as a query and its target, as dbt-lsp-plus gives a compiled model, or as a
statement and its target, and answers each write with its output columns, edges, gaps
or refusal, in the terms of `docs/concepts.md`.

## Uses

| Use | Status | Standard output | Standard error |
|---|---|---|---|
| `static-sql-lineage`, a request on standard input | 0 | the response, as JSON | nothing |
| the same, a request refused as a whole | 2 | nothing | one line, naming the reason |
| `static-sql-lineage --describe` | 0 | the description, as JSON | nothing |
| `static-sql-lineage graph`, its options and paths | 0 | the graph of the scripts' writes, as text or JSON | nothing |
| `static-sql-lineage trace` or `static-sql-lineage uses`, its options, a column and paths | 0 | the column's edges upstream or downstream, as text or JSON | nothing |
| `static-sql-lineage outline`, its options, a write's id and paths | 0 | the write in brief, as text or JSON | nothing |
| any of these, refused | 2 | nothing | one line, naming the reason |
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
another; in T-SQL, which compares names as SQL Server's default collation does,
ignoring case whether they are delimited or not, `orders`, `[Orders]` and `"ORDERS"`
name one relation.

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

## `graph`

The writes of files of SQL, as SQL Server's tools, Snowflake and DuckDB write them, in
the order to migrate them in. Which relations a write reads needs no catalog.

```
static-sql-lineage graph --dialect <name> [--database <name>] [--schema <name>] [--json] <path>...
```

- `--dialect`: required; one of this build's, as `--describe` spells them.
- `--database`, `--schema`: the defaults that complete a name of fewer than three parts,
  as a request's `defaults` do; a database given without a schema takes the dialect's
  usual one, `dbo` in T-SQL, `PUBLIC` in Snowflake, `main` in DuckDB.
- `--json`: JSON rather than text.
- a path: a file, whatever its name, or a directory, whose files ending `.sql`, at any
  depth, are read sorted by path; at least one, read in the order given.

Refused, with status 2, nothing on standard output and one line on standard error: an
option that `graph` does not know, or one without its value; no dialect, or one that the
build lacks; a default that is not one identifier; no path; a path that does not read.

Each file is a script, read in order:

- A byte order mark chooses UTF-8, UTF-16 LE or UTF-16 BE; without one, UTF-8. A file
  that does not decode is a finding, `not_read`, and the others are read.
- In Snowflake and DuckDB, a statement ends at each `;` outside parentheses. In T-SQL, a
  batch ends at a line that holds only `GO`, any case, with an optional count, and at a
  SQLCMD line, whose first token is `:`; a batch that starts `CREATE`, `CREATE OR ALTER`
  or `ALTER`, then `VIEW`, `PROC`, `PROCEDURE`, `FUNCTION` or `TRIGGER`, is one statement;
  any other is split at each `;` outside parentheses, then before each line that starts,
  outside parentheses, with a statement's first word; a piece whose parse fails at its
  last token has run out of text, and grows over the next such line; one where
  polyglot-sql stops after a complete statement at a statement's first word, even within
  a line, as in `IF ... INSERT ...`, ends there.
- A write is a statement that `docs/concepts.md` calls one, other than a bare query. Its
  id is its place, `<path>:<line>:<column>`: the path as given, or as found under a
  directory, then the line and the column of its first token, from 1, counted in
  characters.
- `USE` sets the defaults for the rest of its file: T-SQL's `USE <db>`, Snowflake's
  `USE [DATABASE] <db>`, `USE <db>.<schema>` and `USE SCHEMA [<db>.]<schema>`, DuckDB's
  `USE`; a database without a schema takes the usual one.
- SSMS's existence check, `EXEC sp_executesql` or `EXEC dbo.sp_executesql` with
  `@statement` a national string that parses as one statement, is read as that statement,
  at its `EXEC`'s place; any other dynamic SQL is a finding, `not_read`.
- A `CREATE TABLE` with a column list and no query is a declaration: its relation, as its
  statement names it, and its columns, computed ones included, go to the catalog of the
  commands, in reading order, and it is not counted as skipped. One with no column, or a
  column twice, is a finding, `refused`, its relation left unknown; one that declares its
  relation again is a finding, `declared_twice`, and the later is kept. In T-SQL, its
  storage clauses, which never bear on lineage, are replaced by spaces before it is
  parsed: in its column list, `ROWGUIDCOL`, `SPARSE` and `FILESTREAM` after a column's first
  token; after the list, outside parentheses, `TEXTIMAGE_ON` and `FILESTREAM_ON` with their
  filegroup, `ON` with its filegroup, but not a partition scheme, and `WITH` with its
  options. A `CREATE TABLE` with `AS` outside parentheses is not touched.
- A statement that does not parse is a finding, `refused`, with polyglot-sql's message,
  when its first words make it a write or a declaration; any other statement is skipped,
  and counted.

A write depends on every write of each relation that it reads, other than its own
target, and on the writes of its own target before it. Among the writes whose
dependencies are all ordered, the earliest in reading order comes next; writes that
depend on each other in a cycle come together, at the turn of the earliest, in reading
order, and are a finding, `cycle`, with the relations that they write. A relation with
more than one replace write is a finding, `replaced_twice`.

The text, by default: one line per write, in migration order, its id, its target as its
statement names it, then the relations that it reads, as it names them, without repeats;
one line per finding, after its kind and a colon; then the number of statements skipped:

```
load.sql:1:1 dbo.Fact <- dbo.Stage
load.sql:5:1 dbo.Loop1 <- dbo.Loop2
load.sql:6:1 dbo.Loop2 <- dbo.Loop1
views.sql:3:1 [dbo].[vA] <- dbo.Fact
refused: load.sql:2:1 Expected Into, got Var ('dbo') near [INSERT dbo.Fact (], at 2:8
not_read: legacy.sql not UTF-8, nor UTF-16 after a byte order mark
declared_twice: tables.sql:7:1 [dbo].[Stage]
cycle: load.sql:5:1, load.sql:6:1 (dbo.Loop1, dbo.Loop2)
replaced_twice: [dbo].[vA] (views.sql:3:1, redefine.sql:1:1)
skipped: 3
```

With `--json`, the same as one object:

```json
{
  "writes": [{"id": "load.sql:1:1", "target": "dbo.Fact", "reads": ["dbo.Stage"]}],
  "findings": [
    {"kind": "refused", "place": "load.sql:2:1", "detail": "Expected Into, ..., at 2:8"},
    {"kind": "not_read", "place": "legacy.sql", "detail": "not UTF-8, nor UTF-16 ..."},
    {"kind": "declared_twice", "place": "tables.sql:7:1", "relation": "[dbo].[Stage]"},
    {"kind": "cycle", "writes": ["load.sql:5:1", "load.sql:6:1"], "relations": ["dbo.Loop1", "dbo.Loop2"]},
    {"kind": "replaced_twice", "relation": "[dbo].[vA]", "writes": ["views.sql:3:1", "redefine.sql:1:1"]}
  ],
  "skipped": 3
}
```

- `place`: a place, or for a file that does not decode its path alone. `detail`: text for
  a person, which this contract does not fix; for `refused`, polyglot-sql's message, then
  the line and the column in the file of the token at which it stops, or the rule of the
  catalog that a declaration breaks. `relation`, for `declared_twice`: the relation as the
  later declaration names it.
- The findings of the reading come first, in reading order; then the cycles, in
  migration order; then the relations replaced twice, by their first replace write.

## `trace` and `uses`

A column's edges, followed upstream through the writes that fill its relation to relations
that no write fills, or downstream through the writes that read it.

```
static-sql-lineage trace --dialect <name> [--database <name>] [--schema <name>] [--json] <relation>.<column> <path>...
static-sql-lineage uses --dialect <name> [--database <name>] [--schema <name>] [--json] <relation>.<column> <path>...
```

- The options and the paths are `graph`'s, and the scripts are read as `graph` reads them.
- `<relation>.<column>`: its last part is the column, the rest the relation, each read under
  the dialect, as a request's names are, the relation completed by the defaults: in T-SQL,
  `dbo.vfact.amount` names `[dbo].[vFact].[Amount]`; in Snowflake, `t.b` names `t."B"`, and
  `t."b"` another column.

The writes are answered in migration order, each as protocol 1 answers it, with its
statement's text and the defaults in force there, against the catalog as the scripts'
declarations give it and the writes before it change it. The lineage is the edges of all
the answers, one per pair of columns, where several writes give one pair the first of
`copy`, `derive`, `rows` (R12), with the ids of those writes.

The walk goes from the column, `trace` from children to parents and `uses` from parents to
children, breadth first: each column once, each edge once, at the depth of its first
reaching, from 1. A write is met at a column of its target, for `trace`, or of a relation
that it reads, for `uses`; it gives its refusal, and its gaps on the whole write or on a
column of its target that the walk follows, so that a write that is refused, or whose
upstream is unknown, is not taken for a relation that no write fills. A walk also lists
every refused write of the scripts, a statement whose first words make it a write and that
does not parse, with its place and polyglot-sql's message, and every finding `not_read`,
dynamic SQL or a file that does not decode, with its place and detail: any of them may read
or write the column, and the tool cannot tell which. A cycle is a set of columns that the walk follows,
each of which reaches the others, or one that reaches itself.

Refused, with status 2, nothing on standard output and one line on standard error, as
`graph` is, and for an argument that is not `<relation>.<column>`, or a column that no
declaration, edge or answer's output column names. A column that one names, with no edge
and nothing met, gives an empty line.

The text, by default: one line per edge, in the walk's order, indented by two spaces for
each depth after the first, `<parent> -> <child> <kind> <id>, ...`, each end as
`<relation>.<column>`, with the ids of its writes; then one line per gap and per refusal of
the writes met, one per refused write of the scripts, `refused: <place> <detail>`, its
detail on one line, one per finding not read, `not_read: <place> <detail>`, and one per
cycle, after its kind and a colon:

```
[dbo].[Fact].[Amount] -> dbo.vBase.Total copy views.sql:3:1
  dbo.Stage.Amount -> dbo.Fact.[Amount] copy load.sql:1:1, load.sql:2:1
gap: views.sql:5:1 unknown_relation dbo.Missing -
refused: load.sql:4:1 invalid
refused: views.sql:9:1 Unexpected token: Create, at 10:1
not_read: load.sql:6:1 dynamic SQL
cycle: dbo.Loop1.Id, dbo.Loop2.Id
```

With `--json`, the same as one object, each end a `relation` and a `column`, spelled as an
edge of protocol 1 spells them; a refused write of the scripts is in `refusals` with its
`place` and its `detail`, and no `write`; a finding not read is in `not_read`, with its
`place` and its `detail`:

```json
{
  "column": {"relation": "dbo.vBase", "column": "Total"},
  "edges": [{"parent": {"relation": "[dbo].[Fact]", "column": "[Amount]"},
             "child": {"relation": "dbo.vBase", "column": "Total"},
             "kind": "copy", "writes": ["views.sql:3:1"], "depth": 1}],
  "gaps": [{"write": "views.sql:5:1", "reason": "unknown_relation", "relation": "dbo.Missing", "column": null}],
  "refusals": [{"write": "load.sql:4:1", "reason": "invalid"},
               {"place": "views.sql:9:1", "detail": "Unexpected token: Create, at 10:1"}],
  "not_read": [{"place": "load.sql:6:1", "detail": "dynamic SQL"}],
  "cycles": [[{"relation": "dbo.Loop1", "column": "Id"}, {"relation": "dbo.Loop2", "column": "Id"}]]
}
```

## `outline`

One write in brief: its target, its mode, its place, its answer and the places of its
clauses.

```
static-sql-lineage outline --dialect <name> [--database <name>] [--schema <name>] [--json] <id> <path>...
```

- The options and the paths are `graph`'s; `<id>`, a write's id as `graph` prints it.
- The answer is the write's, as `trace` answers it, in migration order.
- A clause is placed at its first word, among the statement's words outside parentheses and
  `CASE ... END`: `WITH`, as the statement's first word or after `AS`; `SELECT`, `INTO`,
  `FROM`, `WHERE`, `GROUP BY`, `HAVING`, `QUALIFY`, `WINDOW`, `ORDER BY`, `LIMIT`, `UNION`,
  `EXCEPT`, `INTERSECT`, `SET`, `VALUES`, `OUTPUT`, `WHEN`, `ON`, `USING`; and a join, from
  its first word, such as `LEFT OUTER`, to its `JOIN` or `APPLY`. A word in a string or a
  quoted name is none. For SSMS's existence check, the places are in the file, past the
  string's `N'` and each doubled quote.

Refused, with status 2, as `graph` is, and for an id that no write of the scripts has.

The text, by default: one field per line, then the answer's edges, `edge: <parent> ->
<child> <kind>`, its gaps, `gap: <reason> <relation> <column>`, `-` for none, and its
refusal, `refused: <reason> <detail>`, then the clauses, `clause: <words> <place>`:

```
id: views.sql:3:1
target: dbo.vBase
mode: replace
place: 3:1
columns: id, Total
edge: [dbo].[Fact].[Id] -> dbo.vBase.id copy
edge: [dbo].[Fact].[Amount] -> dbo.vBase.Total copy
clause: SELECT 3:26
clause: FROM 3:53
```

`columns` is `-` when the answer has none. With `--json`, one object, whose `answer` is the
write's answer as protocol 1 gives it:

```json
{"id": "views.sql:3:1", "target": "dbo.vBase", "mode": "replace", "place": "3:1",
 "clauses": [{"keyword": "SELECT", "place": "3:26"}, {"keyword": "FROM", "place": "3:53"}],
 "answer": {"write": "views.sql:3:1", "columns": ["id", "Total"],
            "edges": [{"parent": {"relation": "[dbo].[Fact]", "column": "[Id]"},
                       "child": {"relation": "dbo.vBase", "column": "id"}, "kind": "copy"},
                      {"parent": {"relation": "[dbo].[Fact]", "column": "[Amount]"},
                       "child": {"relation": "dbo.vBase", "column": "Total"}, "kind": "copy"}],
            "gaps": [], "refusal": null}}
```

## `--describe`

What this build is:

```json
{
  "tool": "static-sql-lineage",
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

- `version`: the version of `static-sql-lineage`.
- `protocols`: the versions of the request that this build reads.
- `engine`: the SQL engine, with its version and source as `Cargo.lock` pins them. The
  source is given verbatim, so a fork would show its commit.
- `dialects`: the dialects of this build, in this order; a request may name no other.
  A build may leave some out, by its Cargo features.
