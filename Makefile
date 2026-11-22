# The checks of sql-lineage.  Run them from the repository's root in
# `nix develop`, which provides every tool but a C linker.  Four workers by
# default, to keep a laptop cool: `make test CARGO_JOBS=8 TEST_THREADS=8`
# widens them.

CARGO_JOBS ?= 4
TEST_THREADS ?= 4
BASE ?= main

.PHONY: check fmt clippy test kernel-unchanged

check: fmt clippy test

fmt:
	@cargo fmt --check

clippy:
	@cargo clippy --locked -j $(CARGO_JOBS) --all-targets -- -D warnings

test:
	@cargo test --locked -j $(CARGO_JOBS) -- --test-threads=$(TEST_THREADS)

# docs/concepts.md changed only by fillings of homes and operations since the
# branch left BASE, as a plan built while the owner is away may change it.
kernel-unchanged:
	@mkdir -p target
	@git show $$(git merge-base $(BASE) HEAD):docs/concepts.md > target/concepts-at-base.md
	@git show HEAD:docs/concepts.md > target/concepts-at-head.md
	@SQL_LINEAGE_CONCEPTS_BASE=target/concepts-at-base.md SQL_LINEAGE_CONCEPTS_HEAD=target/concepts-at-head.md \
		cargo test --locked -j $(CARGO_JOBS) --test concepts -- --test-threads=$(TEST_THREADS) --ignored --exact kernel_unchanged
