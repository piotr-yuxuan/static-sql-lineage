# Concepts

The kernel of `static-sql-lineage`: the few concepts from which every other term, type
and command is defined. Each entry says what the concept means, what it is not, the
words it rejects, its home in the code, its operations, and scenarios with a verdict. A
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
- home: `Name`, `RelationName`, `Defaults`.
- operations:
  - `Name::read`: reads one identifier, alone, as SQL text under the dialect.
  - `Name::of`: the name of an identifier parsed from a write's SQL, as it spells it.
  - `Name::reported_as`: whether the engine's text for a name is this one's.
  - `RelationName::read`: reads a bare name of one to three parts, completed by the defaults.
  - `RelationName::of`: the name of a table reference parsed from a write's SQL.
  - `RelationName::reported_as`: whether the engine's text for a relation is this one's.
  - `Defaults::read`: reads the request's defaults into the names that complete a name.
  - `Defaults::given`: the defaults that a command gives, a database without a schema taking the dialect's usual one.
  - `Defaults::used`: the defaults after a `USE`.
  - `names_cte`: whether a table reference names a CTE, a step, rather than a relation.
  - `listing`: names, or ends of names, as text separated by `, `.
  - `one_line`: a text, such as polyglot-sql's message in a refusal or a finding, on one line.
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
  over relations, or changes or removes them: `CREATE VIEW` and `ALTER VIEW ... AS`,
  `CREATE TABLE ... AS SELECT`, `CREATE MATERIALIZED VIEW`, a dynamic table,
  `SELECT ... INTO`, `INSERT ... SELECT`, `UPDATE`, `DELETE`, `MERGE`, or a bare query
  given with the target it builds, as dbt-lsp-plus gives a compiled model.
- part, mode: a *replace* write makes its target exactly the query's rows, and the
  query's output columns become the target's columns. Each output column of a replace
  write is named by its query, and no two alike under the dialect, or the write is
  refused. A *contribute* write adds rows to a target that may have other writes, or
  changes values in it, and leaves its columns as they are; its output columns are
  mapped onto the target's by an `INSERT` column list, by position, by `SET`, or by
  `MERGE`'s clauses. A `DELETE` is a contribute write that removes rows.
- part, identity: the id that the caller gives it, unique in the request, which its
  answer echoes. A write read from a script is named by its place,
  `<path>:<line>:<column>`.
- part, location: where the write and each of its clauses start, each a place: a line
  and a column, both from 1, counted in characters, in the SQL text that holds them.
- not: the relation, which may have several writes or none. Not a statement that reads
  no relation: `INSERT ... VALUES`, `TRUNCATE`, or `CREATE TABLE` with a column list.
- avoid: query, which a write contains; statement, since most statements are not
  writes; model, view, producer, definition, transformation, job.
- home: `Write`, `Outputs`, `Holder`, `Key`, `Piece`, `Columns`, `Reading`, `Place`.
- operations:
  - `Write::pieces`: the pieces, each a `SELECT`, that the write's statement is read into, its target and its mode.
  - `Write::read`: a statement's target, mode and pieces, read without the catalog, before their columns are mapped.
  - `Write::text`: the text of a piece of a statement, its own or its tree generated under the dialect.
  - `Write::query`: the query that a piece's text holds, or its refusal.
  - `Write::relations`: the relations that a piece's query reads, as its text names them.
  - `Write::outputs`: the names of a piece's output columns, as its text names them, unknown, or its refusal.
  - `Write::holders`: where each subquery of the write's query sits in the `SELECT` that holds it.
  - `scope_steps`: the steps of a scope's path, each its kind and index, after the path up to it.
  - `Write::steps`: the engine's lineage of each output column, through the query's steps.
  - `Write::keys`: the keys that the query's joins merge, `USING`'s or `NATURAL`'s, in each `SELECT`.
  - `Write::clauses`: the words and the offset of each clause of a statement, its location part.
  - `word`: the text of a token as a word of SQL, none for a quoted name or a string.
  - `Place::at`: the line and the column of an offset in a text of SQL.
- accept `write.contribute-keeps-columns`: catalog `t` with the columns `a` and `b`,
  and `s` with the column `x`; two writes in this order,
  `insert into t (a) select x from s` and `create view u as select a, b from t`: the
  first answer is the edge `s.x -> t.a`, `copy`; the second is the edges
  `t.a -> u.a` and `t.b -> u.b`, both `copy`; no gap in either.
- reject `write.contribute-as-replace`: a gap or a refusal for `b` in the second
  answer, as if the insert had made `a` the only column of `t`.
- accept `write.unnamed-output`: query `select a + 1 from s`: a refusal; no edge.
- reject `write.engine-label`: for that query, an edge to a column that the engine
  named, such as `t._col_0`.
- accept `write.duplicate-output`: query `select a, a as A from s`: a refusal.
- reject `write.quoted-as-duplicate`: a refusal for `select a, a as "a" from s`, whose
  two columns are `A` and `a`.
- accept `write.delete-decides-rows`: catalog `t` with `a` and `b`, `s` with `k`; write
  `delete from t where a in (select k from s)`: the edges `s.k -> t.a` and
  `s.k -> t.b`, `rows`; no gap.
- reject `write.delete-own-condition`: for `delete from t where b < 0`, any edge.

## Edge

- tier: kernel
- means: `(parent relation, parent column) -> (child relation, child column)`, with a
  kind. The child is a write's target and one of its columns; the parent is a relation
  that the write reads, and one of its columns. A write gives at most one edge per pair
  of columns; when several kinds apply, the first of `copy`, `derive`, `rows` is kept. A
  contribute write's clauses reach the columns that it writes, but `DELETE`'s condition
  and `MERGE`'s `ON` reach every column of the target; a condition on the target's own
  row in `UPDATE`, `DELETE` or `MERGE` gives no edge (`decisions.md`, R13).
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
- home: `Edge`, `End`, `Kind`, `Reach`.
- operations:
  - `Kind::of`: the kind of a projection's upstream reference.
  - `Kind::sides`: the kinds of a merged key's two sides, by the kind of the join that merges it.
  - `Reach::of`: where the clauses of one write's query reach.
  - `Reach::rows`: the output columns whose rows a use of a column decides.
  - `Reach::kind`: the kind with which a projection's reference reaches its output column, by the lineage.
  - `End::of`: the end that the engine's names of a relation and of its column resolve to.
  - `End::of_columns`: the ends of a relation's columns, as a write's target gives them.
  - `Edge::of_reference`: the edge from what a reference of the engine reads.
  - `Edge::of_analysis`: the edges and gaps of a write's projections and uses, from the engine's analysis.
  - `Edge::ordered`: a write's edges in protocol 1's order, one per pair of ends.
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
  upstream" is said of named columns; for a contribute write, the target's columns that
  the write reaches.
- part, refusal: the write is not one query or statement that the tool reads, is a
  statement whose target is not the write's, has an output column that its query does
  not name, or two named alike, or is not valid against the catalog, such as a column
  that does not exist. Nothing of it is traced.
- not: a claim that a column has no upstream, unless the answer has no gap and no
  refusal. Unknown is not none.
- avoid: result, report, output; response, which carries all the answers to one
  request.
- home: `Answer`, `Gap`, `GapReason`, `Refusal`, `RefusalReason`.
- operations:
  - `Answer::of`: the answer to one write against the catalog.
  - `Answer::of_piece`: the answer to one piece of a write, merged into the write's.
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
  gives it or, for a command, as its scripts' declarations give it, in reading order:
  each `CREATE TABLE` with a column list is a declared relation, under the rule that a
  request's declared relation follows (`decisions.md`, R15). The writes are read in the
  request's order, or a command's in migration order. After each:
  a replace write sets its target's columns to its output columns; a refused replace
  write, one that does not parse, or one whose `*` leaves its output columns unknown,
  makes its target unknown; a contribute write, refused or not, leaves its target as it
  was (`decisions.md`, R12), and so does a statement that is not a write.
- not: the warehouse: the tool never connects to one and never fetches a shape. Not the
  columns that a query happens to read, which say nothing of the columns a relation
  has.
- avoid: schema, which is also a part of a name; metadata, information schema, sources.
- home: `Catalog`, `Declared`.
- operations:
  - `Catalog::declared`: the catalog that the request declares.
  - `Catalog::declare`: one declared relation, with a column and no column twice, in place of an earlier one.
  - `Catalog::get`: a relation as the catalog knows it, with its columns.
  - `Catalog::schema`: what the engine is told of the known relations that a write reads.
  - `Catalog::record`: what a write makes of its target.
  - `Catalog::answer`: the answer to one write against the catalog as it stands, then what it makes of its target.
- accept `catalog.unknown-relation`: query `select a from r`, with no catalog: an
  `unknown_relation` gap for `r`; no edge.
- reject `catalog.columns-guessed`: for that query, the edge `r.a -> t.a`, as if the
  columns of `r` could be taken from its readers.
- accept `catalog.refused-contribute-keeps-target`: catalog `t` with `a`, `s` with `x`;
  writes `insert into t (nope) select x from s`, then `create view u as select a from t`:
  the first refused, the second the edge `t.a -> u.a`, `copy`.
- reject `catalog.refused-contribute-as-unknown`: an `unknown_relation` gap for `t` in
  the second answer.
- accept `catalog.declared-by-create-table`: a script of `create table s (a int, "B" int);`
  then `create view t as select a, "B" from s;`: `trace t.a` gives the edge `s.a -> t.a`,
  `copy`, and `trace t."B"` the edge `s."B" -> t."B"`, `copy`; no gap.
- reject `catalog.declaration-as-a-write`: for that script, `s` traced as a write's target,
  or an `unknown_relation` gap for `s`.

## Dialect

- tier: kernel
- means: the grammar, and the rules for quoting and case, under which every name and
  every write of one request is read. One per request, and one of the build's. T-SQL's
  rule for case is that of SQL Server's default collation: a name ignores case, whether
  it is delimited or not; a database with another collation is not modelled. Redshift's,
  with `enable_case_sensitive_identifier` off as by default, and Databricks's are as
  T-SQL's; a Redshift database with that setting on is not modelled. PostgreSQL's
  lowers a name unless it is quoted, and a quoted name is exact.
- not: the engine that parses it, polyglot-sql, which can be replaced behind the
  protocol. Not the warehouse product.
- avoid: flavour, engine, adapter, database.
- home: `Dialect`, `Rules`.
- operations:
  - `Dialect::built`: the dialects that the build includes.
  - `Dialect::name`: the dialect's name, as `--describe` spells it.
  - `Dialect::named`: a dialect of the build by its name.
  - `Dialect::polyglot`: the dialect as polyglot-sql names it.
  - `Dialect::strategy`: the rule by which the dialect normalises a name for comparison.
  - `Dialect::polyglot_text`: the text that polyglot-sql reads for a piece's, lowered whole where the dialect ignores case.
  - `Dialect::default_schema`: the schema of a database given without one.
  - `Dialect::keys_first`: whether a `*` over `USING` or `NATURAL` lists the merged keys first.
- accept `dialect.not-built`: a request in `bigquery`, to a build of `snowflake`,
  `tsql` and `duckdb`: refused as a whole, with exit status 2 and no response.
- reject `dialect.generic-fallback`: a response to that request, read as generic SQL.
- accept `dialect.tsql-ignores-case`: dialect `tsql`; catalog `[dbo].[Orders]` with the
  column `[OrderId]`; query `select orderid from dbo.orders`: the edge
  `[dbo].[Orders].[OrderId] -> t.orderid`, `copy`; no gap.
- reject `dialect.tsql-delimited-as-exact`: for that request, a refusal, as if
  `[OrderId]` named only `OrderId` exactly.
- accept `dialect.postgresql-lowers-unquoted`: dialect `postgresql`; catalog `"orders"`
  with the column `"orderid"`; query `select OrderId from Orders`: the edge
  `"orders"."orderid" -> t.OrderId`, `copy`; no gap.
- reject `dialect.postgresql-unquoted-as-exact`: for that request, a refusal or an
  `unknown_relation` gap, as if `Orders` named `"Orders"` or `"ORDERS"`.
- accept `dialect.redshift-ignores-case`: dialect `redshift`; catalog `"Orders"` with
  the column `"OrderId"`; query `select orderid from orders`: the edge
  `"Orders"."OrderId" -> t.orderid`, `copy`; no gap.
- reject `dialect.redshift-quoted-as-exact`: for that request, a refusal or an
  `unknown_relation` gap, as if `"Orders"` named only `Orders` exactly.
- accept `dialect.databricks-ignores-case`: dialect `databricks`; catalog `` `Orders` ``
  with the column `` `OrderId` ``; query `select orderid from orders`: the edge
  `` `Orders`.`OrderId` -> t.orderid ``, `copy`; no gap.
- reject `dialect.databricks-quoted-as-exact`: for that request, a refusal or an
  `unknown_relation` gap, as if `` `Orders` `` named only `Orders` exactly.

## Derived terms

| Term | Defined as | Home |
|---|---|---|
| base relation | a relation that no write of the request has as its target; its columns come from the catalog. Avoid: source | |
| lineage | the edges of all the answers to one request; where several writes give one pair of columns, the first of `copy`, `derive`, `rows` (`decisions.md`, R12) | `Lineage`, `Walk` |
| graph | the relations and the writes between them: a write links its target to each relation its query reads | `Graph`, `Search` |
| trace | a column's edges upstream, followed through earlier writes to base relations | |
| uses | a column's edges downstream | |
| migration order | an order of the writes in which each comes after every write of each relation that it reads, other than its own target, and after the writes of its own target that precede it in reading order; among the strongly connected components of these dependencies whose dependencies are all ordered, the one with the earliest write in reading order comes next, its writes in reading order; a component of more than one write is a cycle, reported with its writes and the relations that they write, and a relation with more than one replace write is reported with those writes | |
| outline | a short form of one write's answer, with its locations, text or JSON | |

## Context

| Term | Defined as | Home |
|---|---|---|
| view, materialised view, dynamic table, table made by `CREATE TABLE ... AS` | a relation with one replace write; the kind of object is a label from its statement | |
| request | one JSON document on standard input: a protocol number, the dialect, a default database and schema, the catalog, and the writes in dependency order | `Request` |
| response | one answer per write of the request, on standard output | `Response` |
| protocol | the number of the request's and response's format; either side refuses another | `Protocol` |
| describe | what the build is: its version, protocols, engine and dialects | `Description`, `Engine` |
| script | a file of SQL statements read in order, as a person or a tool wrote it; what a reader finds in it: writes, statements skipped, statements refused, each with its place | `Script`, `Scripted`, `Finding` |
| command | `graph`, `trace`, `uses` or `outline`, as the command line names it, with its dialect, its defaults and its paths | `Command`, `Verb`, `Cli`, `Options` |

## Kept apart

Look-alike types or functions that stay apart, because they belong to different
concepts or change for independent reasons, one line each: `<one>` and `<other>`:
why. Only the owner adds a line, through a plan.

- `Lineage::of` and `Edge::of_analysis`: both keep one edge per pair of ends, of the
  first kind, but the lineage's rule (R12) spans writes and keeps each edge's write ids,
  where a write's edges are ordered by their child's place.
- `Write::holders` and `Reach::kind`: both know that the second branch of an `INTERSECT`
  or `EXCEPT` decides rows, but read two structures of the engine, the scope path and
  the lineage's `set_branch`.
- `Walk::printed` and the serialisation of `Gap` and `Finding`: a walk lists a gap with
  its write and without its detail, and a refusal or a finding not read by its place and
  detail, as `docs/protocol.md` gives.
- The `refused:` lines of `Walk::printed`, `Command::outline` and `Finding::fmt`: a met
  write's refusal, one write's refusal with its detail, and a refused statement by its
  place; three facts, one word.
- The `cycle:` lines of `Walk::printed` and `Finding::fmt`: columns that reach each
  other, and writes that depend on each other.
- `CUTS` and `WRITES` in `src/script.rs`, and `CLAUSES` in `src/write.rs`: where a T-SQL
  window may end, whether a refused statement was a write, where a clause starts.
- The column lists of `Write::read` and of a declaration in `Script::parsed`: one field
  of the engine, two facts, a write's renamed outputs and a relation's columns.
- `Script::parsed`'s `dbo` and `Dialect::default_schema`'s: the schema under which SSMS
  calls `sp_executesql`, and the schema of a database given without one.
- `tests/protocol/main.rs`'s `outline` and the `Display` of `Edge` and `Gap`: the tests
  take their expectations from the protocol, and cannot link the binary.
- `tests/concepts.rs`'s `repository` and `Command::run`'s walk of a directory: a test's
  reading of `src/`, and a command's reading of scripts with its rule for links.
- `Write::steps` and `Reach::of`: Write gathers the engine's facts about a query, Edge
  places the clauses' reach from them.
- `Gap`'s column and `Lineage::walk`'s reading of it: the gap keeps the spelling, by
  which protocol 1 orders and deduplicates gaps; the walk reads it back as a name.
- The two `Catalog::get(...).map(...)` of `src/write.rs` and `src/answer.rs`: one call
  each, shorter than a function.
- `request_in` and `requests.rs`'s two refusals of a dialect: those tests write the
  field they test.
- The edges of `Walk::printed`'s JSON and the serialisation of `Edge`: the walk names an
  edge's three fields beside its writes and depth, a line shorter than the edge
  serialised with two fields set.
- The empty answers of `Answer::of` and `Answer::of_piece`: one literal each, shorter
  than a function; a `Default` would let any module answer no write.
- The matches of `named`, `with` and `Reach::of` on a query's kinds: polyglot-sql's
  `Union`, `Intersect` and `Except` are three types, and each match reads another field
  beside the `WITH`: the left branch, the limit, the `WITH` to set.
- The patterns of a query in `Write::read` and `Write::query`: one pattern each, where a
  shared test would add a guard to each; the statement's takes a parenthesised query
  whole.
- `Name::reported_as` and `RelationName::reported_as`: one rule over each type's text as
  written, a line each, where a function that both call would add three.
- The catalogs of `case` and `merge` in `tests/protocol/cases.rs`, each built from
  `RELATIONS`: one line each, shorter than a function.

## Not yet modelled

Each needs a fact that the kernel lacks, and joins it, or becomes a derived term,
through the plan for routines (`decisions.md`, Q2 and Q6):

- *routine*, a stored procedure: its branches, and dynamic SQL;
- *temporary relation*, such as `#t` or `@t`: its lifetime;
- *table function*: its parameters;
- *scalar function*: what its body reads.
