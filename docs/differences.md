# Differences

Where the code differs from a definition of `docs/concepts.md`, each difference classed
as a defect of the code, a defect of the definition, an aspiration or an intended gap.

- A T-SQL database whose collation is sensitive to case, or a column with such a
  collation, is not modelled: under T-SQL every name compares as SQL Server's default
  collation compares it, ignoring case. Class: an intended gap.
- Under Snowflake and DuckDB, a table alias read in another case than it was given, as
  `o.a` in `select o.a from s as O`, gives an `unresolved_reference` gap, where the
  Edge entry gives the edge `s.a -> t.a`, `copy`, since the dialect's rules for case
  make `o` and `O` one alias: polyglot-sql looks a qualifier up exactly. Class: a defect
  of the code.
- T-SQL's alias written before its expression, as in `select Total = a from s`, is read
  as a comparison, and the write refused, where the Write entry names its output column
  `Total`: polyglot-sql does not parse that form. Class: a defect of the code.
- A `*` over a source whose columns a list renames, such as `with c (x) as (...)` or a
  derived table with column aliases, leaves a write's output columns unknown, a
  `wildcard` gap, where the Edge entry says that a column selected through `*` is a
  `copy`. Class: an aspiration.
- A write that holds a `PIVOT` gives `analysis_failed` and no edge, where the Edge entry
  says that its grouping columns decide rows: polyglot-sql gives a pivot's grouping
  column no use (#559). Class: a defect of the code.
- A write that holds an `UNPIVOT` gives `analysis_failed` and no edge: polyglot-sql
  traces its name column as a `copy` of the columns it unpivots (#560). Class: a defect
  of the code.
- `TABLE(FLATTEN(...))` is refused `invalid`, where `LATERAL FLATTEN(...)` is traced:
  polyglot-sql does not know the alias of the table function (#557). Class: a defect of
  the code.
- `select * ilike '<pattern>'` is refused `unparsed`: polyglot-sql does not parse it
  (#561). Class: a defect of the code.
- A `*` as a function's argument, such as `object_construct(*)` or `hash(*)`, gives a
  `wildcard` gap, where the Edge entry derives the output column from every column the
  `*` reads: polyglot-sql leaves that column with no upstream (#556). Class: a defect
  of the code.
- A tuple's `IN`, such as `(a, b) in (select x, c from u)`, gives an `unattributed_use`
  gap for its clause, where the Edge entry says that `IN` decides rows: polyglot-sql
  reports only the subquery's first column. Class: a defect of the code.
- An outer column in a scalar subquery's select list, such as `s.b` in
  `(select s.b + max(x) from u)`, gives an `unresolved_reference` gap, where the Edge
  entry derives the output column from it: polyglot-sql reports it as unknown. Class: an
  aspiration.
- A scalar subquery after a `*` in a select list gives an `unattributed_use` gap for the
  uses in it, where the Edge entry places them at the column that holds it: its place
  counts the `*` as one. Class: an aspiration.
- A key that `USING` or `NATURAL` merges across more than two sources, or whose sides
  the engine's lineage does not tell apart, such as a relation joined to itself, gives
  an `unattributed_use` gap for its output column, where the Edge entry gives each key
  the kind of its join's side. Class: an aspiration.
- A key that `USING` or `NATURAL` merges in a branch of a set operation or in a
  subquery leaves the values of each output column whose lineage has a step of the
  key's name to an `unattributed_use` gap, and one merged among sources whose columns
  are unknown those of every output column, where the Edge entry gives each key the
  kind of its join's side: the engine's lineage does not tell its sides there. Class: an
  aspiration.
- A subquery in a field of a `SELECT` other than its select list and the clauses that
  decide rows or order them, such as `LIMIT`, `TOP`, `DISTINCT ON` or a join's pivots,
  gives an `unattributed_use` gap for the uses in it. Class: an aspiration.
- A pivot's `FOR` column is a `derive`, where `definition.md` of the proof of concept
  calls it `rows`: a pivot aggregates a `CASE` on that column, which the Edge entry
  calls a derive. Class: a defect of the definition.
- In a chain of `USING` joins whose middle relation shares a column with the first,
  such as `select * from t join w using (k) join v using (k)`, Snowflake drops the
  middle relation's column from the `*`, where sql-lineage names it and refuses the
  write as `duplicate_column`, as a Snowflake view refuses duplicate names. Class: an
  intended gap.
- T-SQL's `INSERT t (...) SELECT ...`, without `INTO`, as SQL Server's scripts write it,
  and `DELETE FROM t FROM t JOIN ...` are refused `unparsed`, and reported as refused
  writes by `graph`, where the Write entry reads them as writes: polyglot-sql parses
  neither. Class: a defect of the code.
- Snowflake's `INSERT ALL`, which writes several targets, and DuckDB's
  `INSERT ... BY NAME`, which maps its columns by name, are refused `not_a_query`, where
  the Write entry reads `INSERT ... SELECT` as a write. Class: an aspiration.
- Statements that Snowflake refuses and polyglot-sql reads, such as `WITH ... INSERT`,
  `WHEN NOT MATCHED BY SOURCE` in a `MERGE`, or `SELECT ... INTO`, are traced as the
  statements of the dialects that accept them: validating SQL is out of scope. Class: an
  intended gap.
- A subquery in `VALUES`, as in `insert into t values (1, (select max(x) from s))`, reads
  a relation, which the Write entry's *not*, "a statement that reads no relation",
  overlooks; it is refused `not_a_query`. Class: a defect of the definition.
- Under Snowflake, a quoted name that differs from a CTE's only by case or quotes, as
  `"c"` beside `with c as (...)`, names another relation, but polyglot-sql's lineage
  matches CTE names ignoring ASCII case, so a write that reads `"c"` while the catalog
  declares it may take the CTE's columns as its parents, with no gap. Class: a defect of
  the code.
- Dynamic SQL other than SSMS's existence check, such as `EXEC` of a string or of a
  variable, or `sp_executesql` of a variable, is reported by `graph` as not read, where
  the Write entry reads each statement that writes a relation. Class: an aspiration.
- A user's default schema other than the dialect's usual one is known from `--schema`
  alone: `--database` without `--schema`, and a `USE` of a database, take `dbo`,
  `PUBLIC` or `main`, where the Relation entry completes a name with the defaults that
  the SQL is read under. Class: an intended gap.
- In a script in T-SQL, a statement that polyglot-sql refuses ends before the next line
  that starts with a statement's first word, so that its continuation, such as the
  `SELECT` of a refused `INSERT`, is read as a statement of its own, where the Write
  entry reads one statement as one write. A statement also ends where polyglot-sql,
  after a complete statement, stops at a statement's first word within a line, as in
  `IF ... INSERT ...`: that cut relies on polyglot-sql 0.14.3's wording of the error,
  "Invalid expression / Unexpected token", which the test
  `tsql_a_statement_after_an_if_on_its_line_is_read` pins. Class: a defect of the code.
- SQLCMD variables, such as `$(DatabaseName)` in `USE [$(DatabaseName)]`, are left
  unresolved, so that the name that holds one completes the names after it, where the
  Relation entry completes them with the database that the variable names. Class: an
  intended gap.
- In a script in T-SQL, a statement that starts with a word missing from the reader's
  list of statements' first words does not end the statement before it, which is
  refused and reported where it is a write, though the Write entry reads it. Class: a
  defect of the code.
