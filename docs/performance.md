# Performance

How the binary's time and memory grow with the size of a request and of an estate of
scripts, and how it fares on real SQL, so that every plan reports its change in cost
against a recorded baseline. `scripts/bench.py` measures; R22 of `docs/decisions.md`
sets the limit.

## Commands

- `make bench`: the synthetic estates, at `SIZES="100 1000 6000"` by default, into
  `target/bench/results.json`.
- `make bench-corpora`: the real corpora, into `target/bench/corpora.json`. It needs the
  network, for sql-server-samples, and duckdb, for TPC-DS: run it in `nix develop
  .#bench`, the default shell's packages and duckdb.

Each builds the release build, `cargo build --release --locked`, before any run and
outside what it measures; neither is part of `make check`. Each prints a line per way in
as it comes, then the growth. The workflow `.github/workflows/bench.yml` runs both, by
hand and weekly, and keeps both files as the run's artifact; it publishes nothing.

## The estates

An estate is written from a seed, 7, under `target/bench/<size>/<shape>/`, never tracked,
and is synthetic (R5). Its dialect is `duckdb`, whose bare identifiers keep the tokeniser
out of the figures. Every relation has the same W = 20 columns, `a` to `t`; only the
number of writes, the size, grows. Its shapes:

| Shape | Each write |
|---|---|
| `chain` | reads the write before it, the first the one base relation |
| `fan` | reads the one base relation |
| `ctes` | a chain of K = 8 CTEs over an earlier relation |
| `unions` | a union of B = 8 branches, each over an earlier relation |
| `mix` | shaped like a dbt project: the first two fifths, staging, each over one base relation, of a tenth as many as writes; the next two fifths, intermediate, each a join of two staging writes; the rest, marts, each a join of an intermediate write and any earlier one |

An estate is three files: `tables.sql`, the base relations' `CREATE TABLE`, which
declare them to the commands (R15); `writes.sql`, one `CREATE VIEW` per line; and
`request.json`, the request of protocol 1 that gives the same writes, the base relations
as its catalog. The last write is `last`, with a column `a`.

## Ways in

| `way` | What runs |
|---|---|
| `protocol1` | the binary, `request.json` on standard input |
| `graph` | `graph --json` over the scripts |
| `trace`, `uses` | of `last.a` |
| `outline` | of `last`, `writes.sql:<size>:1` |

## Figures

Each way in runs five times. A run's peak resident memory is that child's alone, from
`os.wait4` on its pid: `ru_maxrss`, in KiB on Linux and bytes on macOS. The runner
forks each child rather than letting Popen vfork it, since a child's peak starts from the
memory of what it was copied from: with a fork, the runner's resident memory at that
moment, which is the floor of every peak; with a vfork, the runner's own peak, which
grows over a run. So that the floor stays low, the runner reads a child's output only
after the last run, and holds no output while it forks; so that a peak can be read
against it, the runner records its own resident memory at each fork, from Linux's
`/proc/self/status`, the largest as `floor_kib`. A peak near that floor says little of
the child. A child that exits with a status other than 0, or runs past 300 seconds, ends
the run with status 2 and one line on standard error, and nothing is written.

Each object of the output has these fields:

| Field | Meaning |
|---|---|
| `shape` | the shape, or the corpus |
| `size` | the writes of an estate; of a corpus, its `.sql` files or TPC-DS's queries |
| `way` | the way in |
| `dialect` | the dialect |
| `median_seconds` | the median wall time of the five runs |
| `peak_kib` | the largest peak of the five runs |
| `floor_kib` | the runner's largest resident memory at a fork of these runs; none without `/proc` |
| `version` | the binary's, from `--describe` |
| `writes`, `refusals`, `gaps` | for `protocol1`, from the response; gaps by reason |
| `writes`, `findings`, `skipped` | for `graph`, from its JSON; findings by kind |

The growth, printed for each shape and way in that ran at both 1,000 and 6,000 writes,
is the ratio of the two times.

## Corpora

- `microsoft/sql-server-samples` at the commit `26dc1258b431470f515775b8c98ae77e1d66e39a`,
  fetched into a scratch directory by a sparse checkout, `--no-cone`, of
  `samples/databases/wide-world-importers/**/*.sql`,
  `samples/databases/adventure-works/*-install-script/*.sql` and
  `!**/PostDeploymentScripts/**`, at depth 1 without blobs beyond them, then moved into
  `target/bench/corpora/sql-server-samples`: 368 `.sql` files, 1.2 MB, never the CSV
  data beside them. A fetch that fails, or whose `HEAD` is not that commit, ends the run.
  Read in `tsql` from `samples`: `graph`; `trace` and `uses` of
  `Website.Customers.CustomerID`; `outline` of
  `samples/databases/wide-world-importers/wwi-ssdt/wwi-ssdt/Website/Views/Customers.sql:1:1`.
- TPC-DS, offline, from the `tpcds` extension built into duckdb: with
  `autoinstall_known_extensions` and `autoload_known_extensions` off, once
  `duckdb_extensions()` shows it loaded, else the run ends; the 24 tables' `CREATE
  TABLE` from `CALL dsdgen(sf = 0)` and `duckdb_tables()`, and the 99 queries of
  `tpcds_queries()`, each wrapped as `create view qNN as ...`, under
  `target/bench/corpora/tpcds`. Read in `duckdb` through `protocol1`, the tables as the
  catalog; and as scripts, `tables.sql` then `queries.sql`: `graph`; `trace` and `uses`
  of `q01.c_customer_id`; `outline` of `q01`, `queries.sql:1:1`.

Neither is committed or redistributed (R5).

## A plan's change in cost

On an otherwise idle machine, run `make bench SIZES="1000 6000"` on the plan's base, in a
scratch worktree, and on its tip, back to back, twice, alternating: base, tip, base, tip.
The plan records the four in `docs/work/performance/NNN.json`, untracked, and its
pre-merge summary gives the change.

## The limit

R22, as the owner ruled it: no absolute target yet. No plan worsens the time of a
6,000-write estate, the `protocol1` and `trace` rows of every shape, by more than 10%
without saying why in its pre-merge summary, and the growth from 1,000 to 6,000 writes
stays below 8.
