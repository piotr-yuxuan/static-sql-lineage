# The checks of sql-lineage.  Run them from the repository's root in
# `nix develop`, which provides every tool but a C linker.  Four workers by
# default, to keep a laptop cool: `make test CARGO_JOBS=8 TEST_THREADS=8`
# widens them.

CARGO_JOBS ?= 4
TEST_THREADS ?= 4

.PHONY: check fmt clippy test

check: fmt clippy test

fmt:
	@cargo fmt --check

clippy:
	@cargo clippy --locked -j $(CARGO_JOBS) --all-targets -- -D warnings

test:
	@cargo test --locked -j $(CARGO_JOBS) -- --test-threads=$(TEST_THREADS)
