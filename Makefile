# SPDX-License-Identifier: EUPL-1.2
# The checks of static-sql-lineage.  Run them from the repository's root in
# `nix develop`, which provides every tool but a C linker.  Four workers by
# default, to keep a laptop cool: `make test CARGO_JOBS=8 TEST_THREADS=8`
# widens them.

CARGO_JOBS ?= 4
TEST_THREADS ?= 4
BASE ?= main
# The Python of the release, of the benchmarks and of their tests, fake-gh without
# an extension.
PYTHON := scripts/commands.py scripts/release.py scripts/bench.py tests/tools tests/bin/fake-gh
export PYTHONDONTWRITEBYTECODE := 1

.PHONY: check fmt clippy licence python workflows test-python test kernel-unchanged size bench bench-corpora

check: fmt clippy licence python workflows test-python test

fmt:
	@cargo fmt --check

clippy:
	@cargo clippy --locked -j $(CARGO_JOBS) --all-targets -- -D warnings

# Each tracked source and build file opens with its SPDX line: after `// ` in
# Rust, after `# ` elsewhere, and after the shebang of a script that has one.
# Either list empty, as outside a git repository, is a failure, not a pass.
licence:
	@status=0; rust=$$(git ls-files '*.rs') && build=$$(git ls-files '*.toml' '*.yml' '*.py' tests/bin Makefile flake.nix) \
		&& [ -n "$$rust" ] && [ -n "$$build" ] || { echo "licence: no file to check"; exit 1; }; \
	for file in $$rust; do \
		[ "$$(head -n 1 "$$file")" = "// SPDX-License-Identifier: EUPL-1.2" ] || { echo "$$file: no SPDX line"; status=1; }; \
	done; \
	for file in $$build; do \
		[ "$$(sed -n '1{/^#!/d;};p;q' "$$file")" = "# SPDX-License-Identifier: EUPL-1.2" ] || { echo "$$file: no SPDX line"; status=1; }; \
	done; \
	exit $$status

python:
	@ruff check --no-cache $(PYTHON)

workflows:
	@actionlint

test-python:
	@python3 -m unittest discover -s tests/tools -p '*_test.py'

test:
	@cargo test --locked -j $(CARGO_JOBS) -- --test-threads=$(TEST_THREADS)

# docs/concepts.md changed only by fillings of homes and operations since the
# branch left BASE, as a plan built while the owner is away may change it.
kernel-unchanged:
	@mkdir -p target
	@base=$$(git merge-base $(BASE) HEAD) && git show "$$base":docs/concepts.md > target/concepts-at-base.md
	@git show HEAD:docs/concepts.md > target/concepts-at-head.md
	@SQL_LINEAGE_CONCEPTS_BASE=target/concepts-at-base.md SQL_LINEAGE_CONCEPTS_HEAD=target/concepts-at-head.md \
		cargo test --locked -j $(CARGO_JOBS) --test concepts -- --test-threads=$(TEST_THREADS) --ignored --exact kernel_unchanged

# tokei's count of code, as formatted, which AGENTS.md's budgets read: the Rust
# of src/ and of tests/, and the Python of scripts/ and tests/tools/.
TOKEI_CODE := python3 -c 'import json, sys; print(sys.argv[1], json.load(sys.stdin)["Total"]["code"])'
size:
	@tokei src -t Rust -o json | $(TOKEI_CODE) src
	@tokei tests -t Rust -o json | $(TOKEI_CODE) tests
	@tokei scripts tests/tools -t Python -o json | $(TOKEI_CODE) python

# The benchmarks of docs/performance.md, on the release build, outside `make check`:
# `make bench SIZES="1000 6000"` compares a plan with its base.  The corpora need the
# network and `nix develop .#bench`, which has duckdb.
SIZES ?= 100 1000 6000
bench:
	@cargo build --release --locked -j $(CARGO_JOBS)
	@python3 scripts/bench.py run --sizes $(SIZES)

bench-corpora:
	@cargo build --release --locked -j $(CARGO_JOBS)
	@python3 scripts/bench.py corpora
