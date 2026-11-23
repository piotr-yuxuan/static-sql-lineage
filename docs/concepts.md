# Concepts

The kernel of `sql-lineage`: the few concepts from which every other term, type and
command is defined. Each entry says what the concept means, what it is not, the words
it rejects, its home in the code, its operations, and scenarios with a verdict. A
scenario's identifier is carried by the test that checks it.

- A kernel concept holds a fact that no other concept holds. A derived term holds none:
  it is defined from the kernel, and overlap between derived terms is harmless.
- Every type in the code, a struct, an enum, a union, a trait or a type alias outside
  tests, wherever it is defined and whatever its visibility, belongs to one concept,
  whose `home` line or Home cell names it in backticks. The concept check,
  `tests/concepts.rs`, fails otherwise, on a home that names no type, and on a type's
  name defined twice.
- An entry's `operations` lists, one line each, the functions through which other
  modules should act on the concept, so that code elsewhere calls them and does not
  repeat them. An item opens with the function in backticks, `name` for a free
  function or `Type::name` for one that `Type` defines, visible outside its module and
  outside tests, then says in a few words what it does. The list is chosen, not
  complete. The concept check fails on an item that names no such function.
- A `home` line holds `none yet.` or names in backticks separated by commas, a Home
  cell nothing or such names, and an operations item one line. The concept check
  fails on any other shape.
- This file changes only through a plan whose section *Concepts* declares the change,
  approved by the owner. It is never edited to make a check pass. `AGENTS.md`, under
  Workflow, says which fillings of a home or an operations list need no declaration.
- Definitions are timeless. Where the code differs from one goes in
  `docs/differences.md`, not here.

Tiers: *kernel*; *part of* a kernel concept; *derived*; *context*, words of the SQL read
or of the interface, on which no answer depends.

## Scenarios

Unless a scenario says otherwise, it holds with:

- the dialect `snowflake`;
- one write, `create view t as <query>`, whose target is `t`;
- a catalog that declares each relation the query names, other than `t`, with the
  columns the query mentions, bare.

An accept scenario names every edge of the answer, and the gaps or the refusal it must
have. A reject scenario names an answer that must not be given.

## Relation

- tier: kernel
- means: a named set of rows with named columns that SQL names, because a query reads
  it or a write puts rows into it. Tables, views, materialised views, temporary tables
  and table variables are relations.
- identity: its name, as SQL text read under the request's dialect, completed by the
  request's default database and schema where it names fewer parts, then normalised by
  that dialect's rules for quoting and case. Two spellings that normalise alike name
  one relation, and an edge spells a relation, and each of its columns, as the catalog
  or the write that made it known does.
- part, column: a named column of a relation. Its name is SQL text, read and
  normalised as a part of a relation's name is. A column that a write defines is named
  as the write's query writes it, quoted where it was quoted.
- not: a CTE, a derived table or a subquery, which exist inside one write and are
  steps between the ends of an edge, never an end. Not a dbt node: which node a
  relation is stays the caller's question, since two nodes can name one relation. A
  column is not the label that an unaliased expression gets from the engine, and two
  names are never matched by folding case or stripping quotes outside the dialect's
  rules.
- avoid: object, dataset, node, model, source; view, for relations in general; field
  and attribute, for a column.
- home: none yet.
- operations: none yet.
- accept `relation.cte-is-a-step`: query
  `with orders as (select id from raw.orders) select id from orders`: the edge
  `raw.orders.id -> t.id`, `copy`; no gap.
- reject `relation.cte-is-an-end`: for that query, an edge whose parent is the CTE
  `orders`.
- accept `column.declared-bare`: catalog `s` with the column `amount`; query
  `select AMOUNT as a from s`: the edge `s.amount -> t.a`, `copy`; no gap.
- accept `column.declared-quoted`: catalog `s` with the column `"Amount"`; query
  `select amount as a from s`: a refusal, since `amount` means `AMOUNT`; no edge.
- reject `column.folded-by-hand`: for that query, the edge `s."Amount" -> t.a`.
- accept `relation.completed-by-defaults`: defaults `db` and `s`; catalog `db.s.orders`
  with the column `id`; query `select id from orders`: the edge
  `db.s.orders.id -> t.id`, `copy`; no gap.
- reject `relation.defaults-ignored`: for that request, an `unknown_relation` gap for
  `orders`.
- accept `column.defined-quoted`: two writes in this order,
  `create view u as select a as "Total" from s` and
  `create view v as select "Total" as x from u`: the second answer is the edge
  `u."Total" -> v.x`, `copy`; no gap.
- reject `column.defined-folded`: with `create view v as select total as x from u`
  second, that edge: the write is refused, as `total` means `TOTAL`.
- accept `column.defined-through-star`: with
  `create view u as with c as (select a as "Total" from s) select * from c` first, the
  same edge.

## Write

- tier: kernel
- means: one statement that puts rows into one relation, its *target*, from a query
  over relations: `CREATE VIEW`, `CREATE TABLE ... AS SELECT`,
  `CREATE MATERIALIZED VIEW`, `SELECT ... INTO`, `INSERT ... SELECT`, `MERGE`,
  `UPDATE ... FROM`, or a bare query given with the target it builds, as dbt-lsp-plus
  gives a compiled model.
- part, mode: a *replace* write makes its target exactly the query's rows, and the
  query's output columns become the target's columns. Each output column of a replace
  write is named by its query, and no two alike under the dialect, or the write is
  refused. A *contribute* write adds rows to a target that may have other writes, or
  changes values in it, and leaves its columns as they are; its output columns are
  mapped onto the target's by an `INSERT` column list, by position, by `SET`, or by
  `MERGE`'s clauses.
- part, identity: the id that the caller gives it, unique in the request, which its
  answer echoes. How a write read from a file is named is for the plan for the
  commands.
- part, location: where the write and each of its clauses start, as a line and a
  column in the SQL text that holds them.
- not: the relation, which may have several writes or none. Not a statement that reads
  no relation: `INSERT ... VALUES`, `TRUNCATE`, or `CREATE TABLE` with a column list.
  Whether `DELETE ... WHERE` is a write is open (`decisions.md`, Q4).
- avoid: query, which a write contains; statement, since most statements are not
  writes; model, view, producer, definition, transformation, job.
- home: none yet.
- operations: none yet.
- accept `write.contribute-keeps-columns`: catalog `t` with the columns `a` and `b`,
  and `s` with the column `x`; two writes in this order,
  `insert into t (a) select x from s` and `create view u as select a, b from t`: the
  first answer is the edge `s.x -> t.a`, `copy`; the second is the edges
  `t.a -> u.a` and `t.b -> u.b`, both `copy`; no gap in either. Its test waits on the
  probe of `INSERT` (`decisions.md`, Q1).
- reject `write.contribute-as-replace`: a gap or a refusal for `b` in the second
  answer, as if the insert had made `a` the only column of `t`.
- accept `write.unnamed-output`: query `select a + 1 from s`: a refusal; no edge.
- reject `write.engine-label`: for that query, an edge to a column that the engine
  named, such as `t._col_0`.
- accept `write.duplicate-output`: query `select a, a as A from s`: a refusal.
- reject `write.quoted-as-duplicate`: a refusal for `select a, a as "a" from s`, whose
  two columns are `A` and `a`.

## Edge

- tier: kernel
- means: `(parent relation, parent column) -> (child relation, child column)`, with a
  kind. The child is a write's target and one of its columns; the parent is a relation
  that the write's query reads, and one of its columns. A write gives at most one edge
  per pair of columns; when several kinds apply, the first of `copy`, `derive`, `rows`
  is kept.
- part, kind, with examples:
  - `copy`: on every row, the child's value is the parent's, unchanged, perhaps
    renamed: a column selected or aliased, through `*`, CTEs and subqueries, in each
    branch of a `UNION`.
  - `derive`: the child's value is computed from the parent's: an operator, a cast,
    any function, `coalesce` and `CASE` included, an aggregate, a window function with
    its partition, order and frame, a path into a semi-structured value.
  - `rows`: the parent column decides which rows the child has, or how they group, and
    its value does not reach the child: `WHERE`, a join's condition, `GROUP BY` towards
    the other columns, `HAVING`, `QUALIFY`, `ORDER BY` under a limit, `IN` and
    `EXISTS`, a lateral table function's input.
- The whole catalogue of clauses and their kinds, with the reach of a clause inside a
  scalar subquery, is carried over from the proof of concept as tests, which are its
  home (`decisions.md`, R7).
- not: the dependency of one relation on another as a whole, which belongs to the
  graph and holds whether or not a column flows. Not a step through a CTE or a
  subquery. No edge comes from a column read only by `ORDER BY` without a limit or by
  `DISTINCT`, nor from a literal, `count(*)` or `current_date`, which have no parent
  column.
- avoid: link, flow, dependency, mapping.
- home: none yet.
- operations: none yet.
- accept `edge.alias-is-copy`: query `select id as order_id from s`: the edge
  `s.id -> t.order_id`, `copy`; no gap.
- accept `edge.coalesce-is-derive`: query `select coalesce(a, 0) as a from s`: the
  edge `s.a -> t.a`, `derive`, although dbt's own lineage calls it a copy; no gap.
- accept `edge.order-without-limit`: query `select a from s order by b`: the edge
  `s.a -> t.a`, `copy`; no gap.
- reject `edge.order-is-rows`: for that query, any edge from `s.b`.

## Answer

- tier: kernel
- means: what the tool says of one write: its edges, its gaps and its refusal.
- relates: every write has exactly one answer, which depends on the write, on the
  catalog when the write is read, and on the dialect. The same write read against
  another catalog has another answer (`answer.stale-shape`), so an answer is not a
  part of its write. The answer, in turn, changes the catalog for the writes after it.
- part, gap: a place where an edge may be missing, with its reason: a `*` whose
  columns are unknown (`wildcard`); a relation that the catalog does not know
  (`unknown_relation`); a reference that resolves to nothing known
  (`unresolved_reference`); two relations whose names differ only in case, which the
  engine cannot tell apart, so that the write is not traced (`ambiguous_relation`); a
  clause whose reach the analysis cannot place (`unattributed_use`); an analysis that
  failed (`analysis_failed`).
- part, columns: the write's output columns, when all are known, so that "no
  upstream" is said of named columns.
- part, refusal: the write is not one query that the tool reads, has an output column
  that its query does not name, or two named alike, or is not valid against the
  catalog, such as a column that does not exist. Nothing of it is traced.
- not: a claim that a column has no upstream, unless the answer has no gap and no
  refusal. Unknown is not none.
- avoid: result, report, output; response, which carries all the answers to one
  request.
- home: none yet.
- operations: none yet.
- accept `answer.unknown-star`: query `select * from r`, with no catalog: an
  `unknown_relation` gap for `r` and a `wildcard` gap; no edge.
- reject `answer.silent-none`: for that query, no edge and no gap.
- accept `answer.refusal-spreads`: catalog `s` and `t`, each with the column `a`; two
  writes in this order, `create view t as select nope from s` and
  `create view u as select a from t`: the first answer is a refusal; the second has an
  `unknown_relation` gap for `t`, and no edge.
- reject `answer.stale-shape`: in that second answer, the edge `t.a -> u.a`, taken
  from the catalog's shape of `t`.

## Catalog

- tier: kernel
- means: what is known of relations' names and columns. There is one per request, and
  each write reads it as it stands just before that write. It starts as the request
  gives it, and the writes are read in the request's order. After each:
  a replace write sets its target's columns to its output columns; a refused write, or
  one whose `*` leaves its output columns unknown, makes its target unknown; a
  contribute write leaves its target's columns as they are. What a refused contribute
  write does to its target is open (`decisions.md`, Q3).
- not: the warehouse: the tool never connects to one and never fetches a shape. Not the
  columns that a query happens to read, which say nothing of the columns a relation
  has.
- avoid: schema, which is also a part of a name; metadata, information schema, sources.
- home: none yet.
- operations: none yet.
- accept `catalog.unknown-relation`: query `select a from r`, with no catalog: an
  `unknown_relation` gap for `r`; no edge.
- reject `catalog.columns-guessed`: for that query, the edge `r.a -> t.a`, as if the
  columns of `r` could be taken from its readers.

## Dialect

- tier: kernel
- means: the grammar, and the rules for quoting and case, under which every name and
  every write of one request is read. One per request, and one of the build's.
- not: the engine that parses it, polyglot-sql, which can be replaced behind the
  protocol. Not the warehouse product.
- avoid: flavour, engine, adapter, database.
- home: `Dialect`.
- operations:
  - `Dialect::built`: the dialects that the build includes.
- accept `dialect.not-built`: a request in `bigquery`, to a build of `snowflake`,
  `tsql` and `duckdb`: refused as a whole, with exit status 2 and no response.
- reject `dialect.generic-fallback`: a response to that request, read as generic SQL.

## Derived terms

| Term | Defined as | Home |
|---|---|---|
| base relation | a relation that no write of the request has as its target; its columns come from the catalog. Avoid: source | |
| lineage | the edges of all the answers to one request | |
| graph | the relations and the writes between them: a write links its target to each relation its query reads | |
| trace | a column's edges upstream, followed through earlier writes to base relations | |
| uses | a column's edges downstream | |
| migration order | an order of the writes in which each comes after the writes of the relations it reads; a cycle is reported | |
| outline | a short text form of one write's answer, with its locations | |

## Context

| Term | Defined as | Home |
|---|---|---|
| view, materialised view, dynamic table, table made by `CREATE TABLE ... AS` | a relation with one replace write; the kind of object is a label from its statement | |
| request | one JSON document on standard input: a protocol number, the dialect, a default database and schema, the catalog, and the writes in dependency order | |
| response | one answer per write of the request, on standard output | |
| protocol | the number of the request's and response's format; either side refuses another | |
| describe | what the build is: its version, protocols, engine and dialects | `Description`, `Engine` |

## Kept apart

Look-alike types or functions that stay apart, because they belong to different
concepts or change for independent reasons, one line each: `<one>` and `<other>`:
why. Only the owner adds a line, through a plan.

None yet.

## Not yet modelled

Each needs a fact that the kernel lacks, and joins it, or becomes a derived term,
through the plan for routines (`decisions.md`, Q2 and Q6):

- *routine*, a stored procedure: its branches, and dynamic SQL;
- *temporary relation*, such as `#t` or `@t`: its lifetime;
- *table function*: its parameters;
- *scalar function*: what its body reads.
