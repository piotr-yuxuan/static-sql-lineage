# Differences

Where the code differs from a definition of `docs/concepts.md`, each difference classed
as a defect of the code, a defect of the definition, an aspiration or an intended gap.

- T-SQL's rule for quoting and case is polyglot-sql's `Lowercase`: a bare name folds to
  lower case and a bracketed one keeps its case, so `[OrderId]` is refused against a
  catalog's bare `OrderId`, which SQL Server's default collation, insensitive to case,
  accepts. The Relation entry asks for the dialect's own rules. Class: a defect of the
  code.
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
  and `DELETE FROM t FROM t JOIN ...` are refused `unparsed`, where the Write entry reads
  them as writes: polyglot-sql parses neither. Class: a defect of the code.
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
