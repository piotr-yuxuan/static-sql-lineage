# static-sql-lineage

For every column that a SQL statement writes, static-sql-lineage tells you which
columns of which tables and views it comes from, and how: copied, computed, or only
used to choose the rows. It reads the SQL text alone, with no database, no warehouse
and no running pipeline.

[![Licence](https://img.shields.io/github/license/piotr-yuxuan/static-sql-lineage)](https://github.com/piotr-yuxuan/static-sql-lineage/blob/main/LICENSE)
[![Issues](https://img.shields.io/github/issues/piotr-yuxuan/static-sql-lineage)](https://github.com/piotr-yuxuan/static-sql-lineage/issues)
[![Minimum Rust](https://img.shields.io/badge/rust-1.99%2B-informational)](https://github.com/piotr-yuxuan/static-sql-lineage/blob/main/Cargo.toml)

![static-sql-lineage: the columns of two tables run into a third, marked copy, derive and rows](./docs/social-media-preview.png)

## See it

[`examples/legacy-estate/`](examples/legacy-estate/) holds four T-SQL scripts of a
made-up SQL Server estate. `tables.sql` declares two tables. `load.sql` fills
`dbo.Sales` from `dbo.OrderLine` with `INSERT ... SELECT`. `views.sql` defines two views
over `dbo.Sales`, and `report.sql` one view over those two. Each command below runs from
the root of a clone.

In what order can you port these statements, so that each comes after what it reads?

```console
$ static-sql-lineage graph --dialect tsql examples/legacy-estate
examples/legacy-estate/load.sql:1:1 dbo.Sales <- dbo.OrderLine
examples/legacy-estate/views.sql:1:1 dbo.vRegionRevenue <- dbo.Sales
examples/legacy-estate/views.sql:6:1 dbo.vRegionOrders <- dbo.Sales
examples/legacy-estate/report.sql:1:1 dbo.vRegionReport <- dbo.vRegionOrders, dbo.vRegionRevenue
skipped: 0
```

Each line is a statement, by file, line and column, then the table or view that it
fills, then what it reads. `report.sql` comes before `views.sql` in the folder, but
after it here, since its view reads theirs.

Where does `Revenue` in `dbo.vRegionRevenue` come from?

```console
$ static-sql-lineage trace --dialect tsql dbo.vRegionRevenue.Revenue examples/legacy-estate
dbo.Sales.Amount -> dbo.vRegionRevenue.Revenue derive examples/legacy-estate/views.sql:1:1
dbo.Sales.Region -> dbo.vRegionRevenue.Revenue rows examples/legacy-estate/views.sql:1:1
  dbo.OrderLine.Quantity -> dbo.Sales.Amount derive examples/legacy-estate/load.sql:1:1
  dbo.OrderLine.Status -> dbo.Sales.Amount rows examples/legacy-estate/load.sql:1:1
  dbo.OrderLine.UnitPrice -> dbo.Sales.Amount derive examples/legacy-estate/load.sql:1:1
  dbo.OrderLine.Region -> dbo.Sales.Region copy examples/legacy-estate/load.sql:1:1
  dbo.OrderLine.Status -> dbo.Sales.Region rows examples/legacy-estate/load.sql:1:1
```

Each line is an edge, from one column to another, then how, then the statement that
makes it. `copy` passes the value as it is. `derive` computes a value from it. `rows`
only decides which rows there are, as `WHERE` and `GROUP BY` do. Each indent goes one
step further back, until a table that no script fills.

What would a change to `UnitPrice` in `dbo.OrderLine` reach?

```console
$ static-sql-lineage uses --dialect tsql dbo.OrderLine.UnitPrice examples/legacy-estate
dbo.OrderLine.UnitPrice -> dbo.Sales.Amount derive examples/legacy-estate/load.sql:1:1
  dbo.Sales.Amount -> dbo.vRegionRevenue.Revenue derive examples/legacy-estate/views.sql:1:1
    dbo.vRegionRevenue.Revenue -> dbo.vRegionReport.Revenue copy examples/legacy-estate/report.sql:1:1
    dbo.vRegionRevenue.Revenue -> dbo.vRegionReport.AverageOrder derive examples/legacy-estate/report.sql:1:1
```

What does the statement of `load.sql` do, in brief?

```console
$ static-sql-lineage outline --dialect tsql examples/legacy-estate/load.sql:1:1 examples/legacy-estate
id: examples/legacy-estate/load.sql:1:1
target: dbo.Sales
mode: contribute
place: 1:1
columns: OrderId, Region, Amount
edge: dbo.OrderLine.OrderId -> dbo.Sales.OrderId copy
edge: dbo.OrderLine.Status -> dbo.Sales.OrderId rows
edge: dbo.OrderLine.Region -> dbo.Sales.Region copy
edge: dbo.OrderLine.Status -> dbo.Sales.Region rows
edge: dbo.OrderLine.Quantity -> dbo.Sales.Amount derive
edge: dbo.OrderLine.Status -> dbo.Sales.Amount rows
edge: dbo.OrderLine.UnitPrice -> dbo.Sales.Amount derive
clause: INTO 1:8
clause: SELECT 2:1
clause: FROM 3:1
clause: WHERE 4:1
```

`contribute` says that the statement changes the rows of a table that exists, where a
view replaces what was there. Each clause is placed by line and column. Add `--json` to
any of these commands for output that a program can read.

## Who it is for

**Porting a SQL Server estate to Snowflake.** Whether a person or an agent ports it,
the four commands above answer the questions that come up again and again: what to port
first, where a column comes from, and what a change would reach. The text is short
enough for an agent to read in full, and `--json` gives the same to a program.

**Column lineage for another tool.** A tool that holds SQL, such as the compiled SQL of
a dbt project, sends one JSON request on standard input and reads one JSON response:
for each statement, its edges, and what could not be traced, said so rather than left
out. dbt-lsp-plus, a language server for dbt, plans to use it this way; nothing calls it
yet. [The protocol](docs/protocol.md) gives the request and the response.

## Install

With Rust 1.99 or later and a C linker:

```sh
cargo install --locked --git https://github.com/piotr-yuxuan/static-sql-lineage
```

In a clone, `nix develop` gives Rust. The default build reads Snowflake, T-SQL and
DuckDB, and `static-sql-lineage --describe` says which dialects a build reads.

## What it does not do

- It never connects to a database or a warehouse.
- It knows nothing of dbt or Jinja: give it compiled SQL.
- It does not translate SQL from one dialect to another.
- It does not read stored procedures yet, and counts them as skipped.

## Status and licence

It is pre-release: no version is published yet. To go further,
[the protocol](docs/protocol.md) says how to call it and what it answers,
[the concepts](docs/concepts.md) define its terms, and
[the decisions](docs/decisions.md) say what it is for and what it leaves out.

European Union Public Licence 1.2 ([`LICENSE`](LICENSE)).
