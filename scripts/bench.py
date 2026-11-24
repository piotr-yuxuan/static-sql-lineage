# SPDX-License-Identifier: EUPL-1.2
"""Measure how the binary's time and memory grow with its input; docs/performance.md says how.

    generate --seed S --size N --out DIR
             write one synthetic estate of N writes of each shape into DIR/<shape>: its
             base relations in tables.sql, its writes in writes.sql, one per line, and the
             request of protocol 1 that gives them, request.json

The estates are a pure function of the seed and the size.  Every SQL text is synthetic.
"""

import argparse
import json
import random
import sys
from pathlib import Path

DIALECT = "duckdb"
# W, the columns of every relation; K, the CTEs of a write of the shape `ctes`; B, the
# branches of a write of the shape `unions`.  Only the number of writes grows.
COLUMNS = tuple("abcdefghijklmnopqrst")
CTES = 8
BRANCHES = 8
# The smallest estate: the layers of the shape `mix` need ten writes.
SMALLEST = 10


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


def main() -> int:
    """Run one subcommand: 0 when done, 2 on a failure, said in one line."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    generate = commands.add_parser("generate")
    generate.add_argument("--seed", type=int, required=True)
    generate.add_argument("--size", type=int, required=True)
    generate.add_argument("--out", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        for shape in SHAPES:
            write(arguments.out / shape, estate(shape, arguments.seed, arguments.size))
    except (ValueError, OSError) as error:
        print(f"bench: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
