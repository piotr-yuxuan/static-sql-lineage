# SPDX-License-Identifier: EUPL-1.2
"""Tests of scripts/bench.py: the estates that it writes, as plan 013's D1 shapes them."""

import importlib.util
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

spec = importlib.util.spec_from_file_location("bench", ROOT / "scripts" / "bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

VIEW = re.compile(r"create view (\w+) as (.*);")


def views(files):
    """The writes of an estate's writes.sql, as (name, query) in order."""
    return [VIEW.fullmatch(line).groups() for line in files["writes.sql"].splitlines()]


class Estate(unittest.TestCase):
    def test_constants(self):
        self.assertEqual((len(bench.COLUMNS), bench.CTES, bench.BRANCHES), (20, 8, 8))

    def test_every_shape_has_its_writes_and_a_last_one_with_a_column_a(self):
        self.assertEqual(set(bench.SHAPES), {"chain", "fan", "ctes", "unions", "mix"})
        for shape in bench.SHAPES:
            with self.subTest(shape=shape):
                written = views(bench.estate(shape, 7, 30))
                self.assertEqual(len(written), 30)
                self.assertEqual(len({name for name, _ in written}), 30)
                name, query = written[-1]
                self.assertEqual(name, "last")
                self.assertRegex(query, r" as a, ")

    def test_the_request_gives_the_scripts_writes_and_tables(self):
        for shape in bench.SHAPES:
            with self.subTest(shape=shape):
                files = bench.estate(shape, 7, 20)
                request = json.loads(files["request.json"])
                self.assertEqual((request["protocol"], request["dialect"]), (1, "duckdb"))
                self.assertEqual(
                    [(w["id"], w["sql"]) for w in request["writes"]], [tuple(v) for v in views(files)]
                )
                self.assertTrue(all(w["id"] == w["target"] for w in request["writes"]))
                tables = re.findall(r"create table (\w+) \(([^)]*)\);", files["tables.sql"])
                declared = [(name, re.findall(r"(\w+) int", columns)) for name, columns in tables]
                self.assertEqual([(r["relation"], r["columns"]) for r in request["catalog"]], declared)
                self.assertTrue(all(len(r["columns"]) == 20 for r in request["catalog"]))

    def test_the_same_seed_gives_the_same_files_and_another_seed_others(self):
        for shape in bench.SHAPES:
            with self.subTest(shape=shape):
                self.assertEqual(bench.estate(shape, 7, 20), bench.estate(shape, 7, 20))
                self.assertNotEqual(bench.estate(shape, 7, 20), bench.estate(shape, 8, 20))

    def test_a_chain_reads_the_write_before(self):
        written = views(bench.estate("chain", 7, 20))
        parents = ["base", *(name for name, _ in written[:-1])]
        for (_, query), parent in zip(written, parents, strict=True):
            self.assertTrue(query.endswith(f" from {parent} x"), query)

    def test_a_fan_reads_its_one_base_relation_of_twenty_columns(self):
        files = bench.estate("fan", 7, 20)
        self.assertEqual(len(json.loads(files["request.json"])["catalog"]), 1)
        self.assertTrue(all(query.endswith(" from base x") for _, query in views(files)))

    def test_shapes_of_one_write(self):
        for shape, pattern, count in (("ctes", r"\bt\d as \(select ", 8), ("unions", " union all ", 7)):
            with self.subTest(shape=shape):
                for _, query in views(bench.estate(shape, 7, 20)):
                    self.assertEqual(len(re.findall(pattern, query)), count)

    def test_a_mix_has_staging_then_joins(self):
        written = views(bench.estate("mix", 7, 50))
        self.assertEqual(len(json.loads(bench.estate("mix", 7, 50)["request.json"])["catalog"]), 5)
        self.assertTrue(all(re.search(r" from raw\d+ x$", query) for _, query in written[:20]))
        self.assertTrue(all(" join " in query for _, query in written[20:]))

    def test_refuses_an_estate_too_small(self):
        with self.assertRaises(ValueError):
            bench.estate("mix", 7, 9)
