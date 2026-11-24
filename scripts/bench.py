# SPDX-License-Identifier: EUPL-1.2
"""Measure how the binary's time and memory grow with its input; docs/performance.md says how.

    generate --seed S --size N --out DIR
             write one synthetic estate of N writes of each shape into DIR/<shape>: its
             base relations in tables.sql, its writes in writes.sql, one per line, and the
             request of protocol 1 that gives them, request.json
    run [--sizes N ...]
             time each way in over the estates of each size, written under target/bench,
             with the release build, which it does not build; print each way's figures
             as they come, then write them to target/bench/results.json
    corpora  write TPC-DS from duckdb's built-in tpcds and fetch sql-server-samples' SQL
             at its pinned commit, both under target/bench/corpora, then time each way
             in over them and write target/bench/corpora.json

The estates are a pure function of the seed and the size.  Every SQL text is synthetic.
The figures' median, peak and growth, and the counts read from an output, are pure
functions; the rest runs the binary, alone in each child.

Status 0: done.  Status 2, with one line on standard error: an input is malformed, or a
child or a command failed or ran past TIMEOUT_SECONDS; no figure is recorded.
"""

import argparse
import importlib.util
import json
import os
import random
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = str(ROOT / "target" / "release" / "static-sql-lineage")
BENCH = ROOT / "target" / "bench"
SEED = 7
SIZES = (100, 1000, 6000)
# The runs of each way in, of which the median time is kept; the seconds that one may last.
RUNS = 5
TIMEOUT_SECONDS = 300
# The sizes whose times the growth compares.
GROWTH = (1000, 6000)
# Where Linux says this runner's resident memory, which a forked child's peak starts from.
SELF = Path("/proc/self/status")
DIALECT = "duckdb"
# The SQL of sql-server-samples, never its data, at one commit; and what trace, uses and
# outline are given there, as docs/performance.md pins them.
SAMPLES = "https://github.com/microsoft/sql-server-samples.git"
SAMPLES_COMMIT = "26dc1258b431470f515775b8c98ae77e1d66e39a"
SAMPLES_PATTERNS = (
    "samples/databases/wide-world-importers/**/*.sql",
    "samples/databases/adventure-works/*-install-script/*.sql",
    "!**/PostDeploymentScripts/**",
)
# git without the user's or the system's configuration, hooks or conversion of line endings.
GIT = (
    "env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null"
    " git -c core.autocrlf=false -c core.hooksPath=/dev/null"
).split()
SAMPLES_COLUMN = "Website.Customers.CustomerID"
SAMPLES_WRITE = (
    "samples/databases/wide-world-importers/wwi-ssdt/wwi-ssdt/Website/Views/Customers.sql:1:1"
)
# TPC-DS from the tpcds that duckdb has built in, which nothing may download.
OFFLINE = "SET autoinstall_known_extensions = false; SET autoload_known_extensions = false; "
LOADED = "SELECT loaded FROM duckdb_extensions() WHERE extension_name = 'tpcds'"
TPCDS = (
    "CALL dsdgen(sf = 0); SELECT table_name, sql, (SELECT list(column_name ORDER BY column_index)"
    " FROM duckdb_columns() c WHERE c.table_name = t.table_name) AS columns FROM duckdb_tables() t"
    " ORDER BY table_name; SELECT query_nr, query FROM tpcds_queries() ORDER BY query_nr"
)
# W, the columns of every relation; K, the CTEs of a write of the shape `ctes`; B, the
# branches of a write of the shape `unions`.  Only the number of writes grows.
COLUMNS = tuple("abcdefghijklmnopqrst")
CTES = 8
BRANCHES = 8
# The smallest estate: the layers of the shape `mix` need ten writes.
SMALLEST = 10
# scripts/commands.py, found by this script's path, whether it runs or its tests load it.
_SPEC = importlib.util.spec_from_file_location("commands", Path(__file__).with_name("commands.py"))
commands = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(commands)


def projection(rng: random.Random, aliases: tuple[str, ...]) -> str:
    """Every column of COLUMNS, each copied or derived from the relations under `aliases`."""
    terms = []
    for column in COLUMNS:
        term = f"{rng.choice(aliases)}.{column}"
        if rng.random() < 0.5:
            term += f" + {rng.choice(aliases)}.{rng.choice(COLUMNS)}"
        terms.append(f"{term} as {column}")
    return ", ".join(terms)


def single(rng: random.Random, parent: str) -> str:
    """A query over one relation."""
    return f"select {projection(rng, ('x',))} from {parent} x"


def joined(rng: random.Random, left: str, right: str) -> str:
    """A query over two relations, joined."""
    return f"select {projection(rng, ('x', 'y'))} from {left} x join {right} y on x.a = y.a"


def ctes(rng: random.Random, parent: str) -> str:
    """A query over a chain of CTES CTEs, the first over `parent`."""
    links = [f"t1 as ({single(rng, parent)})"]
    links += [f"t{k} as ({single(rng, f't{k - 1}')})" for k in range(2, CTES + 1)]
    return f"with {', '.join(links)} {single(rng, f't{CTES}')}"


def mix(rng: random.Random, i: int, names: list[str], raws: list[str]) -> str:
    """The write `i` of an estate shaped like a dbt project: staging, intermediate, marts."""
    staged, joining = 2 * len(names) // 5, 4 * len(names) // 5
    if i < staged:
        return single(rng, rng.choice(raws))
    if i < joining:
        return joined(rng, rng.choice(names[:staged]), rng.choice(names[:staged]))
    return joined(rng, rng.choice(names[staged:joining]), rng.choice(names[:i]))


# The query of the write `i`, given the names of all the writes and the base relations.
SHAPES = {
    "chain": lambda rng, i, names, raws: single(rng, names[i - 1] if i else raws[0]),
    "fan": lambda rng, i, names, raws: single(rng, raws[0]),
    "ctes": lambda rng, i, names, raws: ctes(rng, rng.choice(raws + names[:i])),
    "unions": lambda rng, i, names, raws: " union all ".join(
        single(rng, rng.choice(raws + names[:i])) for _ in range(BRANCHES)
    ),
    "mix": mix,
}


def estate(shape: str, seed: int, size: int) -> dict[str, str]:
    """The files of one estate of `size` writes of `shape`, their text by name."""
    if size < SMALLEST:
        message = f"an estate has at least {SMALLEST} writes, not {size}"
        raise ValueError(message)
    rng = random.Random(f"{shape} {seed}")
    names = [*(f"w{i:05}" for i in range(1, size)), "last"]
    raws = [f"raw{i:03}" for i in range(size // 10)] if shape == "mix" else ["base"]
    written = list(zip(names, (SHAPES[shape](rng, i, names, raws) for i in range(size)), strict=True))
    columns = ", ".join(f"{column} int" for column in COLUMNS)
    request = {
        "protocol": 1,
        "dialect": DIALECT,
        "catalog": [{"relation": raw, "columns": list(COLUMNS)} for raw in raws],
        "writes": [{"id": n, "target": n, "sql": q} for n, q in written],
    }
    return {
        "tables.sql": "".join(f"create table {raw} ({columns});\n" for raw in raws),
        "writes.sql": "".join(f"create view {n} as {q};\n" for n, q in written),
        "request.json": json.dumps(request) + "\n",
    }


def write(directory: Path, files: dict[str, str]) -> None:
    """Write `files`, their text by name, into `directory`."""
    directory.mkdir(parents=True, exist_ok=True)
    for name, text in files.items():
        (directory / name).write_text(text)


class BenchError(commands.CommandError):
    """A child that failed, or a command that printed what was not expected."""


def figures(samples: list[tuple[float, int]], platform: str) -> tuple[float, int]:
    """The median seconds and the largest peak in KiB of the runs of one way in.

    Each run is its seconds and its `ru_maxrss`, which macOS gives in bytes and Linux in KiB.
    """
    scale = 1024 if platform == "darwin" else 1
    return statistics.median(s for s, _ in samples), max(m // scale for _, m in samples)


def counts(way: str, output: bytes) -> dict:
    """What a way in's output counts: writes, refusals and gaps by reason, or findings."""
    if way == "protocol1":
        answers = json.loads(output)["answers"]
        return {
            "writes": len(answers),
            "refusals": sum(answer["refusal"] is not None for answer in answers),
            "gaps": dict(Counter(gap["reason"] for answer in answers for gap in answer["gaps"])),
        }
    if way == "graph":
        graph = json.loads(output)
        return {
            "writes": len(graph["writes"]),
            "findings": dict(Counter(finding["kind"] for finding in graph["findings"])),
            "skipped": graph["skipped"],
        }
    return {}


def growth(rows: list[dict]) -> dict[tuple[str, str], float]:
    """For each shape and way that ran at both sizes of GROWTH, the ratio of their times."""
    times = {(row["shape"], row["way"], row["size"]): row["median_seconds"] for row in rows}
    small, large = GROWTH
    return {
        (shape, way): seconds / times[shape, way, small]
        for (shape, way, size), seconds in times.items()
        if size == large and (shape, way, small) in times
    }


def call(*command: str) -> bytes:
    """What a command prints; any failure stops the run."""
    return commands.output(command, TIMEOUT_SECONDS)


def ways(dialect: str, paths: list[str], column: str, write: str, request: Path | None) -> list:
    """Each way in to time, its arguments and its standard input: protocol 1, if `request`
    is given, then the commands over the scripts at `paths`."""
    options = {"graph": ["--json"], "trace": [column], "uses": [column], "outline": [write]}
    found = [("protocol1", [BINARY], request)] if request else []
    for way, rest in options.items():
        found.append((way, [BINARY, way, "--dialect", dialect, *rest, *paths], None))
    return found


def measure(label: dict, way: str, argv: list[str], cwd: Path, stdin: Path | None) -> dict:
    """The figures of RUNS runs of the binary by one way in, under `label`."""
    runs, floor = [], 0
    for run in range(RUNS):
        with (
            open(stdin or os.devnull, "rb") as given,
            tempfile.TemporaryFile() as out,
            tempfile.TemporaryFile() as err,
        ):
            if SELF.exists():
                floor = max(floor, int(SELF.read_text().split("VmRSS:")[1].split()[0]))
            start = time.perf_counter()
            # A child starts with the resident memory of what it was copied from: a vfork,
            # which Popen uses without a preexec_fn, would give it this runner's peak, a
            # fork only this runner's resident memory now.
            process = subprocess.Popen(
                argv, cwd=cwd, stdin=given, stdout=out, stderr=err, preexec_fn=os.getpid
            )
            signal.signal(signal.SIGALRM, lambda *_, pid=process.pid: os.kill(pid, signal.SIGKILL))
            signal.alarm(TIMEOUT_SECONDS)
            # wait4 gives this child's usage alone; RUSAGE_CHILDREN keeps the largest so far.
            _, status, usage = os.wait4(process.pid, 0)
            seconds = time.perf_counter() - start
            signal.alarm(0)
            process.returncode = code = os.waitstatus_to_exitcode(status)
            if code != 0:
                err.seek(0)
                said = " ".join(err.read().decode(errors="replace").split())
                message = f"{' '.join(argv)} in {cwd}: status {code} after {seconds:.0f} s: {said}"
                raise BenchError(message)
            runs.append((seconds, usage.ru_maxrss))
            # Only the last output is read, so that none is resident while a child is forked.
            if run == RUNS - 1:
                out.seek(0)
                output = out.read()
    seconds, peak = figures(runs, sys.platform)
    row = {**label, "way": way, "median_seconds": round(seconds, 4), "peak_kib": peak}
    row |= {"floor_kib": floor or None} | counts(way, output)
    found = " ".join(f"{key} {row[key]}" for key in ("writes", "refusals", "skipped") if key in row)
    line = f"{row['shape']:<18} {row['size']:>6} {way:<9} {seconds:>9.3f} s {peak:>9} KiB  {found}"
    print(line, f"floor {floor} KiB", flush=True)
    return row


def report(rows: list[dict], path: Path) -> None:
    """Write `rows` to `path` as JSON, then print their growth."""
    path.write_text(json.dumps(rows, indent=1) + "\n")
    for (shape, way), ratio in growth(rows).items():
        print(f"growth {shape} {way} {ratio:.1f}")


def run(sizes: list[int]) -> None:
    """Time each way in over the estates of each of `sizes`."""
    version = json.loads(call(BINARY, "--describe"))["version"]
    rows = []
    for size in sizes:
        for shape in SHAPES:
            directory = BENCH / str(size) / shape
            write(directory, estate(shape, SEED, size))
            label = {"shape": shape, "size": size, "dialect": DIALECT, "version": version}
            paths, outline = ["tables.sql", "writes.sql"], f"writes.sql:{size}:1"
            for way, argv, stdin in ways(DIALECT, paths, "last.a", outline, directory / "request.json"):
                rows.append(measure(label, way, argv, directory, stdin))
    report(rows, BENCH / "results.json")


def corpora() -> None:
    """Write TPC-DS and fetch sql-server-samples, then time each way in over each."""
    version = json.loads(call(BINARY, "--describe"))["version"]
    root = BENCH / "corpora"
    if call("duckdb", "-jsonlines", "-c", OFFLINE + LOADED).strip() != b'{"loaded":true}':
        raise BenchError("this duckdb has no tpcds built in")
    dumped = call("duckdb", "-jsonlines", "-c", OFFLINE + TPCDS)
    found = [json.loads(line) for line in dumped.splitlines() if line]
    tables = [row for row in found if "table_name" in row]
    queries = {f"q{row['query_nr']:02}": row["query"] for row in found if "query_nr" in row}
    queries = {name: query.strip().rstrip(";") for name, query in queries.items()}
    request = {
        "protocol": 1,
        "dialect": DIALECT,
        "catalog": [{"relation": table["table_name"], "columns": table["columns"]} for table in tables],
        "writes": [{"id": name, "target": name, "sql": query} for name, query in queries.items()],
    }
    tpcds = root / "tpcds"
    write(tpcds, {
        "tables.sql": "".join(table["sql"] + "\n" for table in tables),
        "queries.sql": "".join(f"create view {name} as {query};\n" for name, query in queries.items()),
        "request.json": json.dumps(request) + "\n",
    })
    # Fetched apart, then moved into place, so that a fetch cut short leaves nothing behind.
    samples, scratch = root / "sql-server-samples", tempfile.mkdtemp(dir=root)
    try:
        for command in (
            ["init", "-q"],
            ["remote", "add", "origin", SAMPLES],
            ["sparse-checkout", "set", "--no-cone", *SAMPLES_PATTERNS],
            ["fetch", "-q", "--depth", "1", "--filter=blob:none", "origin", SAMPLES_COMMIT],
            ["checkout", "-q", "FETCH_HEAD"],
        ):
            call(*GIT, "-C", scratch, *command)
        head = call(*GIT, "-C", scratch, "rev-parse", "HEAD").decode().strip()
        if head != SAMPLES_COMMIT:
            raise BenchError(f"sql-server-samples is at {head}, not {SAMPLES_COMMIT}")
        shutil.rmtree(samples, ignore_errors=True)
        os.rename(scratch, samples)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    rows = []
    for label, cwd, paths, column, outline, given in (
        ({"shape": "tpcds", "size": len(queries), "dialect": DIALECT}, tpcds,
         ["tables.sql", "queries.sql"], "q01.c_customer_id", "queries.sql:1:1", tpcds / "request.json"),
        ({"shape": "sql-server-samples", "size": len(list(samples.rglob("*.sql"))), "dialect": "tsql"},
         samples, ["samples"], SAMPLES_COLUMN, SAMPLES_WRITE, None),
    ):
        for way, argv, stdin in ways(label["dialect"], paths, column, outline, given):
            rows.append(measure(label | {"version": version}, way, argv, cwd, stdin))
    report(rows, BENCH / "corpora.json")


def main() -> int:
    """Run one subcommand: 0 when done, 2 on a failure, said in one line."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    subcommands = parser.add_subparsers(dest="command", required=True)
    generate = subcommands.add_parser("generate")
    generate.add_argument("--seed", type=int, required=True)
    generate.add_argument("--size", type=int, required=True)
    generate.add_argument("--out", type=Path, required=True)
    subcommands.add_parser("run").add_argument("--sizes", type=int, nargs="+", default=SIZES)
    subcommands.add_parser("corpora")
    arguments = parser.parse_args()
    try:
        if arguments.command == "run":
            run(arguments.sizes)
        elif arguments.command == "corpora":
            corpora()
        else:
            for shape in SHAPES:
                write(arguments.out / shape, estate(shape, arguments.seed, arguments.size))
    except (commands.CommandError, ValueError, KeyError, OSError, subprocess.TimeoutExpired) as error:
        print(f"bench: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
