# Protocol

How a caller runs `sql-lineage`, and what it answers. The request and the response,
protocol 1, come with tracing, at step 2 of the widening in `decisions.md`.

## Uses

| Use | Status | Standard output | Standard error |
|---|---|---|---|
| `sql-lineage --describe` | 0 | the description, as JSON | nothing |
| any other, no argument included | 2 | nothing | one line, naming what was given |

## `--describe`

What this build is:

```json
{
  "tool": "sql-lineage",
  "version": "0.0.0",
  "protocols": [],
  "engine": {
    "name": "polyglot-sql",
    "version": "0.14.3",
    "source": "registry+https://github.com/rust-lang/crates.io-index"
  },
  "dialects": ["snowflake", "tsql", "duckdb"]
}
```

- `version`: the version of `sql-lineage`.
- `protocols`: the versions of the request that this build reads; none yet.
- `engine`: the SQL engine, with its version and source as `Cargo.lock` pins them. The
  source is given verbatim, so a fork would show its commit.
- `dialects`: the dialects of this build, in this order; a request may name no other.
  A build may leave some out, by its Cargo features.
