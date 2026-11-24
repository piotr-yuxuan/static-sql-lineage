# static-sql-lineage

For every column that a SQL statement writes, static-sql-lineage tells you which
columns of which tables and views it comes from, and how: copied, computed, or only
used to choose the rows. It needs nothing but the SQL text: no database, no warehouse,
no running pipeline.

[![CI](https://github.com/piotr-yuxuan/static-sql-lineage/actions/workflows/ci.yml/badge.svg)](https://github.com/piotr-yuxuan/static-sql-lineage/actions/workflows/ci.yml)
[![CD](https://github.com/piotr-yuxuan/static-sql-lineage/actions/workflows/cd.yml/badge.svg)](https://github.com/piotr-yuxuan/static-sql-lineage/actions/workflows/cd.yml)
[![Latest release](https://img.shields.io/github/v/release/piotr-yuxuan/static-sql-lineage?sort=semver)](https://github.com/piotr-yuxuan/static-sql-lineage/releases/latest)
[![Commits since the latest release](https://img.shields.io/github/commits-since/piotr-yuxuan/static-sql-lineage/latest?sort=semver)](https://github.com/piotr-yuxuan/static-sql-lineage/commits/main)
[![Last commit](https://img.shields.io/github/last-commit/piotr-yuxuan/static-sql-lineage)](https://github.com/piotr-yuxuan/static-sql-lineage/commits/main)
[![Licence](https://img.shields.io/github/license/piotr-yuxuan/static-sql-lineage)](https://github.com/piotr-yuxuan/static-sql-lineage/blob/main/LICENSE)
[![Issues](https://img.shields.io/github/issues/piotr-yuxuan/static-sql-lineage)](https://github.com/piotr-yuxuan/static-sql-lineage/issues)
[![Minimum Rust](https://img.shields.io/badge/rust-1.99%2B-informational)](https://github.com/piotr-yuxuan/static-sql-lineage/blob/main/Cargo.toml)
[![Dialects](https://img.shields.io/badge/dialects-Databricks%20%7C%20DuckDB%20%7C%20PostgreSQL%20%7C%20Redshift%20%7C%20Snowflake%20%7C%20T--SQL-blue)](https://github.com/piotr-yuxuan/static-sql-lineage/blob/main/docs/protocol.md)
[![SQL engine](https://img.shields.io/badge/SQL%20engine-polyglot--sql-informational)](https://crates.io/crates/polyglot-sql)
[![Nix flake](https://img.shields.io/badge/nix-flake-5277C3?logo=nixos&logoColor=white)](https://github.com/piotr-yuxuan/static-sql-lineage/blob/main/flake.nix)

![static-sql-lineage: the columns of two tables run into a third, marked copy, derive and rows](./docs/social-media-preview.png)

## See it

[`examples/tpch/`](examples/tpch/) holds five DuckDB scripts over the eight tables of
TPC-H, the standard benchmark of orders, customers and suppliers. `tables.sql` declares
the tables. `staging.sql` gives them plain names, `marts.sql` adds up orders, customers
and suppliers, `reports.sql` builds what the business reads, and `loads.sql` fills a
table of revenue by day. The tool needs only their text; to run them on data, DuckDB's
`tpch` extension makes the tables with `CALL dbgen(sf = 0.01)`. Each command below runs
from the root of a clone, and `--dialect duckdb` tells it to read the scripts as
DuckDB's SQL.

In what order should these statements run, so that each comes after the tables and
views that it reads?

```console
$ static-sql-lineage graph --dialect duckdb examples/tpch
examples/tpch/staging.sql:3:1 stg_line <- lineitem
examples/tpch/staging.sql:16:1 stg_order <- orders
examples/tpch/marts.sql:3:1 order_revenue <- stg_line, stg_order
examples/tpch/staging.sql:25:1 stg_customer <- customer, nation, region
examples/tpch/loads.sql:3:1 revenue_by_day <- stg_customer, stg_line, stg_order
examples/tpch/marts.sql:15:1 customer_revenue <- order_revenue, stg_customer
examples/tpch/reports.sql:3:1 nation_revenue <- customer_revenue
examples/tpch/reports.sql:8:1 region_report <- nation_revenue
examples/tpch/reports.sql:13:1 top_customers <- customer_revenue
examples/tpch/staging.sql:36:1 stg_supplier <- nation, supplier
examples/tpch/marts.sql:27:1 supplier_margin <- partsupp, stg_line, stg_supplier
examples/tpch/marts.sql:37:1 nation_party <- stg_customer, stg_supplier
skipped: 0
```

Each line is a statement: its file, line and column, then the table or view that it
fills, then what it reads. The order is not the files' order: `marts.sql` and
`reports.sql` come before `staging.sql` in the folder, yet each of their views waits
for the views that it reads. `tables.sql` only declares tables, so it has no line. The
last line counts the statements that the tool skipped, such as stored procedures, which
it does not read yet.

Where does `revenue` in `order_revenue` come from?

```console
$ static-sql-lineage trace --dialect duckdb order_revenue.revenue examples/tpch
stg_line.net_price -> order_revenue.revenue derive examples/tpch/marts.sql:3:1
stg_line.order_key -> order_revenue.revenue rows examples/tpch/marts.sql:3:1
stg_line.returned -> order_revenue.revenue rows examples/tpch/marts.sql:3:1
stg_order.customer_key -> order_revenue.revenue rows examples/tpch/marts.sql:3:1
stg_order.order_date -> order_revenue.revenue rows examples/tpch/marts.sql:3:1
stg_order.order_key -> order_revenue.revenue rows examples/tpch/marts.sql:3:1
  lineitem.l_discount -> stg_line.net_price derive examples/tpch/staging.sql:3:1
  lineitem.l_extendedprice -> stg_line.net_price derive examples/tpch/staging.sql:3:1
  lineitem.l_linestatus -> stg_line.net_price rows examples/tpch/staging.sql:3:1
  lineitem.l_linestatus -> stg_line.order_key rows examples/tpch/staging.sql:3:1
  lineitem.l_orderkey -> stg_line.order_key copy examples/tpch/staging.sql:3:1
  lineitem.l_linestatus -> stg_line.returned rows examples/tpch/staging.sql:3:1
  lineitem.l_returnflag -> stg_line.returned derive examples/tpch/staging.sql:3:1
  orders.o_custkey -> stg_order.customer_key copy examples/tpch/staging.sql:16:1
  orders.o_orderstatus -> stg_order.customer_key rows examples/tpch/staging.sql:16:1
  orders.o_orderdate -> stg_order.order_date copy examples/tpch/staging.sql:16:1
  orders.o_orderstatus -> stg_order.order_date rows examples/tpch/staging.sql:16:1
  orders.o_orderkey -> stg_order.order_key copy examples/tpch/staging.sql:16:1
  orders.o_orderstatus -> stg_order.order_key rows examples/tpch/staging.sql:16:1
```

Each line is an edge from one column to another, then its kind, then the statement
that makes it. `copy` passes the value as it is. `derive` computes a new value from it,
as `sum(l.net_price)` does. `rows` only decides which rows there are, as `WHERE`,
`JOIN ... ON` and `GROUP BY` do. Each indent goes one step further back, until a table
that no script fills, here `lineitem` and `orders`. A column of a report has a longer
story: `trace region_report.revenue` goes five steps back.

What would a change to `l_discount` in `lineitem` reach?

```console
$ static-sql-lineage uses --dialect duckdb lineitem.l_discount examples/tpch
lineitem.l_discount -> stg_line.net_price derive examples/tpch/staging.sql:3:1
lineitem.l_discount -> stg_line.gross_price derive examples/tpch/staging.sql:3:1
  stg_line.net_price -> order_revenue.revenue derive examples/tpch/marts.sql:3:1
  stg_line.net_price -> revenue_by_day.revenue derive examples/tpch/loads.sql:3:1
  stg_line.net_price -> supplier_margin.margin derive examples/tpch/marts.sql:27:1
    order_revenue.revenue -> customer_revenue.revenue derive examples/tpch/marts.sql:15:1
      customer_revenue.revenue -> nation_revenue.revenue derive examples/tpch/reports.sql:3:1
      customer_revenue.revenue -> top_customers.region rows examples/tpch/reports.sql:13:1
      customer_revenue.revenue -> top_customers.customer rows examples/tpch/reports.sql:13:1
      customer_revenue.revenue -> top_customers.revenue copy examples/tpch/reports.sql:13:1
      customer_revenue.revenue -> top_customers.position derive examples/tpch/reports.sql:13:1
        nation_revenue.revenue -> region_report.revenue derive examples/tpch/reports.sql:8:1
        nation_revenue.revenue -> region_report.average_order derive examples/tpch/reports.sql:8:1
```

The walk goes the other way here: each indent goes one step further forward, through
the margins, the nightly load, the ranking of the top customers, and the report by
region.

What does `region_report` do, in brief?

```console
$ static-sql-lineage outline --dialect duckdb examples/tpch/reports.sql:8:1 examples/tpch
id: examples/tpch/reports.sql:8:1
target: region_report
mode: replace
place: 8:1
columns: region, revenue, average_order
edge: nation_revenue.region -> region_report.region copy
edge: nation_revenue.region -> region_report.revenue rows
edge: nation_revenue.revenue -> region_report.revenue derive
edge: nation_revenue.orders -> region_report.average_order derive
edge: nation_revenue.region -> region_report.average_order rows
edge: nation_revenue.revenue -> region_report.average_order derive
clause: SELECT 9:1
clause: FROM 10:1
clause: GROUP BY 11:1
```

The statement is named by the id that `graph` printed. `mode: replace` says that it
replaces whatever was there, as a `CREATE VIEW` does; an `INSERT` would say
`contribute`, since it changes the rows of a table that is already there. Each `clause`
gives the line and the column where a keyword starts. Add `--json` to any of these
commands for output that a program can read.

## Who it is for

**Porting a SQL Server estate to Snowflake.** Whether a person or an agent does the
port, the four commands above answer the questions that come up again and again: what
to port first, where a column comes from, what a change would reach, and what one
statement does. They read T-SQL as SQL Server's tools script it: `GO` batches,
bracketed names, statements without semicolons, UTF-16 files.
[`examples/legacy-estate/`](examples/legacy-estate/) is a small estate in T-SQL:

```console
$ static-sql-lineage graph --dialect tsql examples/legacy-estate
examples/legacy-estate/load.sql:1:1 dbo.Sales <- dbo.OrderLine
examples/legacy-estate/views.sql:1:1 dbo.vRegionRevenue <- dbo.Sales
examples/legacy-estate/views.sql:6:1 dbo.vRegionOrders <- dbo.Sales
examples/legacy-estate/report.sql:1:1 dbo.vRegionReport <- dbo.vRegionOrders, dbo.vRegionRevenue
skipped: 0
```

The text is short enough for an agent to read in full, and `--json` gives the same to a
program.

**Column lineage for another tool.** A tool that has SQL in hand, such as a dbt
project's compiled SQL, sends one JSON request on standard input and reads one JSON
response: each statement's edges, and a note of whatever it could not trace, so that
unknown is never taken for none. dbt-lsp-plus, a language server for dbt, plans to use
it this way, but nothing calls it yet. [The protocol](docs/protocol.md) gives the
request and the response.

## Install

With Rust 1.99 or later and a C linker:

```sh
cargo install --locked --git https://github.com/piotr-yuxuan/static-sql-lineage
```

In a clone, `nix develop` gives Rust, and `cargo install --locked --path .` installs
from it. The default build reads Snowflake, T-SQL, DuckDB, PostgreSQL, Redshift and
Databricks; `static-sql-lineage --describe` says which dialects a build reads.

`static-sql-lineage --help` lists the commands, and `static-sql-lineage help trace`
gives one in full. A shell completes the commands, the options and the dialects once it
loads the script that `completions` prints, for instance from its startup file:

```sh
source <(static-sql-lineage completions bash)    # in ~/.bashrc
source <(static-sql-lineage completions zsh)     # in ~/.zshrc, after compinit
static-sql-lineage completions fish | source     # in ~/.config/fish/config.fish
```

## What it does not do

- It never connects to a database or a warehouse.
- It knows nothing of dbt or Jinja: give it compiled SQL.
- It does not translate SQL from one dialect to another.
- It does not read stored procedures yet, and counts them as skipped.

## Status and licence

It is early: each change to `main` that passes its checks is published as a
[release](https://github.com/piotr-yuxuan/static-sql-lineage/releases), the first being
0.1.0. Its licence is the European Union Public Licence 1.2 ([`LICENSE`](LICENSE)).

## Further reading

- [The protocol](docs/protocol.md): how to call it, from a shell or a program, and what
  it answers.
- [The concepts](docs/concepts.md): the terms that it uses, defined.
- [The decisions](docs/decisions.md): what it is for, and what it leaves out.
