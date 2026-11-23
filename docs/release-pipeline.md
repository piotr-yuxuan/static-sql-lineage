# The release pipeline

Two workflows of GitHub Actions and one script, after dbt-lsp-plus's. Neither workflow holds a
check of its own: the checks are in the `Makefile`, and the release in `scripts/release.py`.

## The workflow CI

`.github/workflows/ci.yml` runs on a push to any branch, and on a pull request from a fork, so
that no branch is checked twice. A newer run of the same reference cancels the one under way.
Its permission is `contents: read`. Its one job, `check`, runs `nix develop -c make check` on
`ubuntu-24.04`, with Nix from `cache.nixos.org` and this repository's own Actions cache, and has
30 minutes. The `Makefile`'s cap of four workers stays, as GitHub's runners have no more. The dev
shell takes the C linker (`cc`) from the runner, as it does on a laptop.

## The workflow CD

`.github/workflows/cd.yml` runs in two cases, and never in a fork:

- once CI completes on `main`: it goes on only if CI's conclusion is `success`, its event `push`,
  and its commit from this repository;
- by hand on `main`, with an input `bump`: `patch`, the default, `minor` or `major`.

So an automatic release raises the patch part. Its permissions are `contents: write` and `actions:
read`, with the job's own `github.token` and no secret. Runs take one at a time: the one that runs
is never cancelled, and a newer one takes the place of the one that waits; it has 15 minutes. It
checks out the commit that CI checked, with every tag, and runs `python3 scripts/release.py`. The
runner's Python and `gh` serve: it needs no Nix.

## What is cached

CI's Nix store, keyed on the runner's system and a hash of `flake.nix` and `flake.lock`, with the
newest store of the same system restored failing that; and cargo's registry and `target/`, keyed
on the job, the system, and a hash of `Cargo.lock` and `flake.lock`, which fixes rustc. Each
release rewrites `Cargo.lock`'s version line, so the next push's key is new and starts from the
newest cache of the job, then saves its own: the caches grow until GitHub evicts the oldest. A
branch reads its own caches and `main`'s, and never writes `main`'s. No third-party binary cache,
no `--accept-flake-config` and no change to `substituters`. CD has no cache.

## What the release writes

The version is the package's: `Cargo.toml`'s `[package] version` and the package's own entry in
`Cargo.lock`, the one without `source`, which must agree. The tag is `X.Y.Z`.

| Write | What it is |
|---|---|
| The version commit on `main` | Through GitHub's API, so that GitHub signs it. It sets the version in both files and changes no other byte. Its message is `Version X.Y.Z`, a blank line and `[skip ci]`, so that no CI follows it |
| The release | `X.Y.Z`, with its tag on that commit. Its notes are the table of the files changed since the release before, and the compare link. A first release has neither |

The version is written only by the pipeline. The bump raises its part and sets the parts after it
to zero, but no release is below 0.1.0: from `Cargo.toml`'s 0.0.0, the first release is 0.1.0,
and the next push's is 0.1.1.

## What a run decides

A run reads first, and makes only the writes that are missing.

| What it finds | What it does | Status |
|---|---|---|
| `main` is at the commit | Both writes | 0 |
| `main` has moved, and the next commit is a version commit whose release exists | Nothing | 0 |
| `main` has moved, and the next commit is a version commit with no release | The release alone: a run cut short finishes | 0 |
| `main` has moved, and the next commit is no version commit | Nothing, as the newer push's run releases | 0 |
| `main` is at a version commit with no release, by hand | The release alone, and no bump: run it again for that | 0 |
| By hand, and CI has not succeeded on `main`'s head, or on its parent when the head is a version commit | Nothing | 2 |
| A version that is malformed, or that the two files do not agree on, or not found exactly once in either; the next version's tag exists; a version commit's tag is on another commit | Nothing | 2 |
| git or GitHub refuses a call, takes over 120 s, or answers something else | Nothing more. If `main` moved just after the read, GitHub refuses the commit, and the newer push's run releases | 2 |

A version commit is told by its message, its one parent, and what it changes: `Cargo.toml` and
`Cargo.lock`, and nothing else.

## What the owner sets on GitHub

Nothing, as the repository stands: Actions may run every action, and a workflow's own
`permissions` give CD its `contents: write` whatever the default for workflows. Rules on `main`,
if any are added, must let `github-actions[bot]` commit through the API.

## Raise a version by hand

Open **Actions**, then **CD**, then **Run workflow**, on `main`, and choose the `bump`. Or:

```sh
gh workflow run cd.yml --ref main -f bump=minor
```

## Keep a clone in step

Each release adds a commit to `origin/main`. So a local `main` runs `git pull --ff-only` before its
next merge, or the push of that merge is refused.
