# Working in this repository

This repository holds `sql-lineage`, a command-line tool in Rust on polyglot-sql. For
each SQL write, it answers which columns of which relations reach which columns of the
write's target, and how. It knows SQL and nothing of dbt. `docs/decisions.md` says
what it is for and what it leaves out; `docs/concepts.md` defines its terms.

## Workflow

Every change follows the skill `workflow:plans`; load it before the first edit. In
this repository: plans are `docs/work/plans/NNN-slug.md`, not tracked; the check is
`make check`, run in `nix develop`.

Each plan has a section *Concepts*, which the review of its draft checks against
`docs/concepts.md` and against the other open plans:

```markdown
## Concepts

Adds: <concepts or derived terms>
Changes: <and how>
Retires: <and what replaces them>
Relies on: <concepts used unchanged>
```

- Every type belongs to a concept whose `home` names it. The concept check, the test
  `tests/concepts.rs` that `make check` runs, fails otherwise, and each plan's
  acceptance criteria include it, so that a type outside `docs/concepts.md` is a
  blocking finding.
- `docs/concepts.md` changes only in a plan that declares the change, and the
  pre-merge summary shows the diff of its kernel. It is never edited to make a check
  pass.
- Where the code differs from a definition goes in `docs/differences.md`, created at
  the first difference, each classed as a defect of the code, a defect of the
  definition, an aspiration or an intended gap.
- Tests take their expectations from `docs/concepts.md`, the protocol and the
  catalogue of clauses carried over from the proof of concept, never from the
  implementation; a scenario's test carries its identifier.
- The diagrams in `docs/diagrams/` are derived from `docs/concepts.md` and
  `docs/decisions.md`, drawn with the skill `diagram-design:diagram-design`; a plan
  that changes what one shows redraws it.
- Tracked SQL is synthetic.

## The kernel

```
Relation  a named set of rows with named columns that SQL names
  column  part of a relation; its name is SQL text under the dialect
Write     one statement that puts rows into one relation from a query over relations
Edge      (parent relation, column) -> (child relation, column): copy, derive or rows
Answer    what is said of one write: its edges, its gaps and its refusal
Catalog   what is known of relations' names and columns when a write is read
Dialect   the grammar and the rules for quoting and case of one request
```

Words to avoid: object, node, dataset; source, for a base relation; model and view,
for relations in general; query, for a write; field, link, result.

## Layout

```
docs/decisions.md  what the tool is for: users, rulings, promises, scope, open questions
docs/concepts.md   the kernel, its derived terms, and scenarios with their verdicts
docs/diagrams/     views of the two documents above: the kernel, the use cases
docs/protocol.md   how a caller runs the binary, and what it answers
docs/work/         plans and their notes, not tracked
src/               one module per home of `docs/concepts.md`
tests/concepts.rs  the concept check
tests/describe.rs  --describe, and the uses the binary refuses
```
