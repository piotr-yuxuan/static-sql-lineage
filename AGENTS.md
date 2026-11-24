# Working in this repository

This repository holds `static-sql-lineage`, a command-line tool in Rust on
polyglot-sql. For each SQL write, it answers which columns of which relations reach
which columns of the write's target, and how. It knows SQL and nothing of dbt.
`docs/decisions.md` says what it is for and what it leaves out; `docs/concepts.md`
defines its terms.

## Workflow

Every change follows the skill `workflow:plans`; load it before the first edit. In
this repository: plans are `docs/work/plans/NNN-slug.md`, not tracked; the check is
`make check`, run in `nix develop`.

Each plan has a section *Concepts*, which the review of its draft checks against
`docs/concepts.md`, against the other open plans, and, for its line *Reuses*, against
the code at the base:

```markdown
## Concepts

Adds: <concepts or derived terms>
Changes: <and how>
Retires: <and what replaces them>
Relies on: <concepts used unchanged>
Reuses: <operations and types at the base that the steps call or extend>
```

- Every type belongs to a concept whose `home` names it. The concept check, the test
  `tests/concepts.rs` that `make check` runs, fails otherwise, and each plan's
  acceptance criteria include it, so that a type outside `docs/concepts.md` is a
  blocking finding.
- `docs/concepts.md` changes only in a plan that declares the change, and the
  pre-merge summary shows the diff of its kernel. It is never edited to make a check
  pass. Adding to an existing concept's `home` or `operations` a type or a function
  that serves it is not a change to declare. The pre-merge summary lists each, with why
  it serves that concept, and the review checks the reasons.
- Where the code differs from a definition goes in `docs/differences.md`, created at
  the first difference, each classed as a defect of the code, a defect of the
  definition, an aspiration or an intended gap.
- Tests take their expectations from `docs/concepts.md`, the protocol and the
  catalogue of clauses carried over from the proof of concept, never from the
  implementation. A scenario's row carries its identifier, and a failing row names it:
  the macro names its test after it, `.` and `-` becoming `_`.
- The diagrams in `docs/diagrams/` are derived from `docs/concepts.md` and
  `docs/decisions.md`, drawn with the skill `diagram-design:diagram-design`; a plan
  that changes what one shows redraws it.
- Tracked SQL is synthetic.
- A budget per plan: while a plan is drafted, three estimators, on the same models each
  round (Opus, Sonnet, Opus), read the plan and the documents at its base, never the
  code, and estimate the lines it adds to `src/` and `tests/`. A budget is a ceiling,
  never a target: fewer lines are better, and no line is written to reach it. It is the
  estimators' median scaled by the ratio of our code to their median whole at the last
  check, so that it is in our own density, and its ceiling is that figure plus 20% or
  plus 50 lines, whichever is more. `NNN-checks.sh` prints the count and fails above
  the ceiling; only the owner raises a budget.
- The check of every three plans: three blind estimators of the whole feature set give
  its median. The ratio of our count to that median, for `src/` and for `tests/`, rises
  by no more than 0.05 from one check to the next, the noise between rounds (0.746 and
  0.814 at plan 012; later ratios in the chain's index of their check). A ratio that
  rises more makes the next plan a consolidation that brings it back down. The ratios
  found are the next budgets' scale. A budget in Python is scaled by our Python's ratio
  to the same code at ruff's default width, 0.820 at plan 012 (972 lines against
  1,186), taken again at each check; the rule of 0.05 covers `src/` and `tests/` only.
- `make size` gives the counts, so that no count is taken by hand.
- The review's findings (step 5 of `workflow:plans`) are recorded with
  `coordinator:instrument` in `docs/work/findings/NNN.json`, and its summary is quoted
  in the pre-merge summary.
- A new dependency is named in its plan's *Decisions*.

## Reuse before repetition

- Before writing a function or a type, look for one with the same job in the code at
  the base: in the `operations` lists of `docs/concepts.md`, then in codegraph's index
  where it is available, else in the signatures of the modules concerned. A worktree
  has an index of its own, made by `codegraph init` when the worktree is added, so
  the index answers from the branch, not from the primary checkout.
- A change to existing code that lets the request reuse it, rather than repeat it,
  traces to the request. It extracts what the existing use and the new one share,
  rather than adding a flag or a branch; it is a commit of its own, right before the
  commit that uses it; and the existing tests pass unchanged. Until that next commit
  the extracted code has one caller, which the rule against single-use helpers allows.
  The pre-merge summary lists each such commit with the tests that cover the code it
  moved. This rule prevails over the shared rule that every changed line traces to the
  request, and over the Karpathy guideline *Surgical changes*.
- A rule that several places must agree on lives in one type whose constructor
  applies it and whose fields are private, so that no place builds or compares one
  otherwise; the names of relations and columns are the first case, as the Relation
  entry of `docs/concepts.md` says.
- Each plan's acceptance criteria include one for the review, manual since no command
  can check it: no new function or type repeats behaviour that exists at the base,
  unless *Kept apart* in `docs/concepts.md` lists the two, or the plan's notes propose
  them for it, with why; a repetition only proposed blocks the merge until the owner
  rules. This rule prevails over the reviewer's default preference for duplication
  over a wrong abstraction.

## Plans built while the owner is away

A plan that the owner approves to be built in their absence, as in a chain of plans
overnight:

- waits on no open question of `docs/decisions.md`: each one that it needs is closed
  before the plan is approved;
- only adds to `docs/concepts.md`, and only names to `home` lines and Home cells and
  items to `operations` lists, in the shapes that file gives. Its `NNN-checks.sh`
  runs `make kernel-unchanged`, which compares the file with its state where the
  branch began and fails otherwise, and the review of the plan's draft checks that it
  does. A plan that needs more, such as a type that serves no existing concept, stops
  and leaves its branch.

Beyond homes and operations, such a plan may change `docs/concepts.md` only by a text
that the owner approved for it, or by retiring names that the owner named, written as
approved; its checks then compare the file with that text instead of running
`make kernel-unchanged`.

When such a chain ends, an agent whose only task is to find behaviour repeated across
`src/` and `tests/`, rather than in one diff, writes what it finds to
`docs/work/consolidation/`, which the owner reads with the chain's results. Each
repetition becomes a step of the next plan drafted after it or, on the owner's ruling,
a line of *Kept apart*.

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
LICENSE              the EUPL 1.2
README.md            what the tool tells a user, and its examples, which a test runs
examples/            synthetic scripts that the README reads: tpch, of DuckDB over TPC-H's
                     tables, and legacy-estate, of SQL Server
docs/decisions.md    what the tool is for: users, rulings, promises, scope, open questions
docs/concepts.md     the kernel, its derived terms, and scenarios with their verdicts
docs/differences.md  where the code differs from a definition, each difference classed
docs/diagrams/       views of the two documents above: the kernel, the use cases
docs/protocol.md     how a caller runs the binary, and what it answers
docs/release-pipeline.md
                     CI and CD: what each runs, and what a release writes and decides
docs/performance.md  how the binary's time and memory are measured, and the limit on them
docs/work/           plans and their notes, not tracked
src/                 one module per home of `docs/concepts.md`: relation, write, edge,
                     answer, catalog, request, dialect, describe, script (files read
                     as scripts), graph (the migration order), command (the command
                     line and what it prints)
tests/concepts.rs    the concept check
tests/protocol/      the binary run as a caller runs it: --describe and the uses it
                     refuses, requests, scenarios, answers, edges, stars, catalogue (of
                     clauses), cases (Snowflake's), agreement (with dbt's lineage),
                     statements (replace and contribute), commands (graph on scripts,
                     and the README's examples), logging (-v and the variables); rows,
                     the macro of rows, which tests/concepts.rs loads too
tests/scripts/       synthetic scripts as SQL Server's tools, Snowflake and DuckDB write
                     them, kept byte for byte by `.gitattributes`: CRLF, UTF-16; and in
                     expected/, what commands print from them, written from the protocol
tests/agreement/     the fixture and the probes, synthetic: a request, dbt's edges, and
                     the explanations of each difference
tests/tools/         the tests of scripts/release.py and scripts/bench.py; tests/bin/fake-gh
                     stands in for gh
scripts/commands.py  what a command prints, or why it failed, for the scripts beside it
scripts/release.py   CD's release: the version commit, the release and its notes
scripts/bench.py     the benchmarks: synthetic estates, the corpora, and their figures
.github/workflows/   CI, `make check` on every push; CD, a release after each green push
                     to main; bench.yml, the benchmarks, weekly and by hand
```
