# SPDX-License-Identifier: EUPL-1.2
# The checks of sql-lineage.  Run them from the repository's root in
# `nix develop`, which provides every tool but a C linker.  Four workers by
# default, to keep a laptop cool: `make test CARGO_JOBS=8 TEST_THREADS=8`
# widens them.

CARGO_JOBS ?= 4
TEST_THREADS ?= 4
BASE ?= main

.PHONY: check fmt clippy licence test kernel-unchanged

check: fmt clippy licence test

fmt:
	@cargo fmt --check

clippy:
	@cargo clippy --locked -j $(CARGO_JOBS) --all-targets -- -D warnings

# Each tracked source and build file opens with its SPDX line: after `// ` in
# Rust, after `# ` elsewhere. Either list empty, as outside a git repository,
# is a failure, not a pass.
licence:
	@status=0; rust=$$(git ls-files '*.rs') && build=$$(git ls-files '*.toml' Makefile flake.nix) \
		&& [ -n "$$rust" ] && [ -n "$$build" ] || { echo "licence: no file to check"; exit 1; }; \
	for file in $$rust; do \
		[ "$$(head -n 1 "$$file")" = "// SPDX-License-Identifier: EUPL-1.2" ] || { echo "$$file: no SPDX line"; status=1; }; \
	done; \
	for file in $$build; do \
		[ "$$(head -n 1 "$$file")" = "# SPDX-License-Identifier: EUPL-1.2" ] || { echo "$$file: no SPDX line"; status=1; }; \
	done; \
	exit $$status

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
