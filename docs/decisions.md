# Decisions

What `sql-lineage` is for, what it promises and what it leaves out, as ruled by the
owner. A ruling changes only through a plan that the owner approves. An open question
names the probe or the ruling that closes it, and the plan that must wait for it.

## What it is

`sql-lineage` reads SQL that writes relations and answers, for each write, which
columns of which relations reach which columns of its target, and how. It knows SQL and
nothing of dbt, and it never connects to a database (see "Promises"). Its terms are
those of `concepts.md`.

## Use cases

| Use case | Caller | Interface | Relies on |
|---|---|---|---|
| U1. Column lineage of a dbt project's compiled models: for dbt-oss users, who have no other, and compared with dbt's own where the proprietary build writes it | dbt-lsp-plus, and dbt-mode through it | one JSON request on standard input, one response on standard output; `--describe` | replace writes, the catalog, answers |
| U2. Porting a legacy SQL Server estate to Snowflake: which relations each write reads, the order to migrate them in, a column's sources and readers, one write in brief | an agent, or a person | `graph`, `trace`, `uses`, `outline`: short text by default, JSON on request | every kind of write, routines, the derived terms; later, the linter's own concepts |
| U3. Refactoring a dbt project: duplicated logic and conflicting rules, found by fingerprints of units of logic rooted through `copy` edges | the owner, or an agent | none before its own plan | edges, and concepts that its plan declares |

## Rulings

Ruled on 2026-11-22.

- R1. Scope: lineage first. Other commands join this tool rather than tools of their
  own, one at a time, each through a plan that declares its concepts, in the order of
  "Widening" below.
- R2. Objects: any write, not views alone. Single-statement writes first; routines and
  functions after a probe of how the engine reads them.
- R3. Dialects: Snowflake and T-SQL, and DuckDB for the fixtures carried over. A
  dialect that the build lacks is refused. Adding one changes no concept: it is a
  feature of the engine's build, a name in `--describe`, and its scenarios.
- R4. Engine: polyglot-sql from crates.io, behind the protocol, so that it can be
  replaced. The tool is written in Rust.
- R5. Evidence: tracked SQL is synthetic. Real projects are read locally, and nothing
  of them is committed.
- R6. The kernel is `concepts.md`. A plan declares the concepts it adds, changes,
  retires and relies on. The concept check, the test `tests/concepts.rs` that
  `make check` runs, refuses a type that maps to no concept.
- R7. The proof of concept's evidence is carried over as tests: the definition of an
  edge with its catalogue of clauses, the protocol, the 25 Snowflake cases, and the
  comparisons with dbt's lineage on the fixture, the probes and jaffle-shop. On
  polyglot-sql 0.14.3, the proof of concept passed 20 of the 25 cases. Two of the five
  others, quoted identifiers and an unquoted column against a quoted one, declare
  `Amount` bare to mean the exact name: under the kernel's rule they are rewritten to
  declare `"Amount"`, as the column scenarios of `concepts.md` do, and must pass.
  `PIVOT`, `UNPIVOT` and `IDENTIFIER()` are carried as known failures. The code is not
  copied: each part is rebuilt by a plan, test first.
- R8. Plans and their notes are untracked, under `docs/work/`, as in the owner's other
  repositories.

Ruled on 2026-11-22, later.

- R9. Licence: EUPL-1.2, the text in `LICENSE`, and an SPDX line in each source and
  build file, which `make check` keeps.
- R10. Name: `sql-lineage`, kept when the tool widens to the other use cases.
- R11. Names of relations: a request may give a default database and schema, and every
  partly qualified name of its catalog and writes is completed with them, under the
  dialect's rules, before any comparison.

Ruled on 2026-11-23.

- R12. Several writes to one relation (Q3): the relation's lineage is the union of its
  writes' edges, and where two writes give one pair of columns different kinds, the first
  of `copy`, `derive`, `rows` is kept; a contribute write that is refused leaves its
  target as it was.
- R13. `DELETE` and `MERGE`'s matching condition (Q4): `DELETE ... WHERE` or `USING`,
  and `MERGE`'s `ON`, give `rows` edges to every column of the target from other
  relations' columns, and none from the target's own; `DELETE` is a contribute write.
  Stored procedures are set aside.

Taken with plan 006 under the owner's standing word, for the owner to see: a statement
whose own target is not its write's is refused (D1); a condition of a `MERGE`'s `WHEN`
clause decides the rows of the columns that its action writes, and of no other (D4); a
value that a column takes from its own row, as in `SET a = a + 1`, is an edge (D5); a
contribute write to a target that the catalog does not know gives an `unknown_relation`
gap for it, and the edges of the columns that its statement names (D6); the catalog
after a write follows its statement's kind (D7).

Taken with plan 007 under the owner's standing word, for the owner to see:

- R14. T-SQL's names compare as SQL Server's default collation compares them, ignoring
  case, whether bare, bracketed or double-quoted: `[dbo].[Customer]` and
  `dbo.customer` name one relation. A database with another collation is not modelled.

## Widening, in order

Each step reuses what the steps before it built, so that the cheapest concepts come
first and the commands that share least with lineage come last. Only the next plan is
drafted at a time, and the owner may reorder for value.

1. The skeleton: the crate, `make check`, the concept check and `--describe`. No new
   concept.
2. Protocol 1 and replace writes: the request read, refused at its boundary and
   traced, with the evidence of R7 and the names of R11, in two plans: protocol 1 with
   each write given as a query and its target, then the rest of R7's evidence. Q7 is
   closed (see "Provenance"). Reading a request without tracing it would need a stub
   answer or fields that nothing reads, so the two come together. Serves U1.
3. Contribute writes; Q1 probed, and Q3 and Q4 ruled first. Serves U1 and U2.
4. `graph`, `trace`, `uses` and `outline`, from answers alone: derived terms, no new
   kernel concept; Q9 first. Serves U2.
5. Routines and functions: the facts that `concepts.md` lists as not yet modelled; Q2
   probed and Q6 ruled first. Serves U2.
6. Duplicated logic: units of logic and their fingerprints, rooted through `copy`
   edges. Serves U3.
7. A linter of migration traps and design smells: findings read from the syntax tree
   rather than from answers. Serves U2.
8. A check of a translation against a warehouse: the only command that would connect
   to a database, against the promise below, so its plan first decides whether it
   belongs here. Serves U2.

## Promises

The protocol keeps these promises of the proof of concept's contract. Its shape, a
catalog and writes, is step 2's, as protocol 1, since nothing reads the proof of
concept's yet.

- One request in, one response out, then exit: status 0 with a response; status 2,
  with a message on standard error, for a request refused as a whole, such as
  malformed JSON, an unknown field, another protocol, a dialect that the build lacks,
  or a relation's name that is not a bare name.
- Writes come in dependency order, and each changes the catalog as `concepts.md`
  says: a replace write's output columns become its target's for the writes after it;
  a refused replace write, or a `*` that leaves its output unknown, makes its target
  unknown; a contribute write, refused or not, leaves its target as it was.
- Names are SQL text, settled by the dialect on both sides of the protocol. An edge
  spells a relation and its columns as the catalog or the write that made it known
  does.
- Unknown is not none: gaps and refusals are explicit.
- No dbt, no Jinja, no network, no warehouse. The same request gives the same
  response.

These hold until a plan that the owner approves says otherwise; step 8 is the only one
foreseen to ask.

## Out of scope

- Rendering Jinja, and anything of dbt: nodes, manifests, compilation, which build
  compiled the SQL.
- Translating between dialects, except as step 8 may decide.
- Validating SQL for its own sake: lineage validates only to decide what it may trace.
- Column types, formatting, performance, parity of data.

## Open questions

| # | Question | Closed by | Before step |
|---|---|---|---|
| Q2 | How it reads T-SQL routines and functions: `BEGIN ... END`, `#t` and `@t`, `IF ... ELSE`, dynamic SQL run by `EXEC`, inline and multi-statement table functions, `CROSS APPLY`, a scalar function whose body reads a table | a probe | 5 |
| Q6 | Whether every branch of a routine counts, so that lineage says what may happen; whether answers about a routine show its temporary relations or pass through them | the owner | 5 |
| Q8 | Whether the repository will be public | the owner | the first release |
| Q9 | Whether a `CREATE TABLE` with a column list feeds the catalog, for the porting use, which reads files rather than a request | the owner | 4 |

## Provenance

The evidence is in dbt-lsp-plus's untracked `docs/work/`: in `column-lineage/`,
`definition.md` (edges, kinds and answers), `findings.md` (the protocol),
`investigation-2026-11-22.md` (polyglot-sql 0.14.3), `cases/`, `agreement/` and
`snowflake-trial/`; and `migration-tools.md` for the porting of legacy views.

Q7, whether a defined column's exact case can be carried to later writes, was closed
by two probes, untracked in this repository's `docs/work/probes/`:
`2026-11-22-q7-output-quoting/` and `2026-11-23-plan-004-names/`. The parsed tree says
whether each output name was quoted, at its place in the outermost select list, and
through a `*` in what the `*` reads; a replace write with an unnamed output column is
refused, as Snowflake refuses it, and so is one with two columns named alike.

Q1, how polyglot-sql reads writes other than a query, was closed by the probe
`2026-11-23-plan-006-statements/`, untracked in `docs/work/probes/`, with `ALTER VIEW`
and `SELECT ... INTO` over a set operation from `2026-11-23-plan-007-files/`: what each
statement parses into in each dialect, how its target and its query are read, and the
synthetic `SELECT`s of `UPDATE`, `DELETE` and `MERGE`, checked on Snowflake.
