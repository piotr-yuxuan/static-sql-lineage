# Differences

Where the code differs from a definition of `docs/concepts.md`, each difference classed
as a defect of the code, a defect of the definition, an aspiration or an intended gap.

- T-SQL's rule for quoting and case is polyglot-sql's `Lowercase`: a bare name folds to
  lower case and a bracketed one keeps its case, so `[OrderId]` is refused against a
  catalog's bare `OrderId`, which SQL Server's default collation, insensitive to case,
  accepts. The Relation entry asks for the dialect's own rules. Class: a defect of the
  code.
- A clause inside a subquery, such as the `WHERE` of an `EXISTS` or an `IN` subquery,
  gives an `unattributed_use` gap, where the Edge entry says that `IN` and `EXISTS`
  decide rows: polyglot-sql does not say which projection or clause holds a subquery.
  Plan 005 places it from the tree. Class: an aspiration.
- A `*` leaves a write's output columns unknown, a `wildcard` gap, where the Edge entry
  says that a column selected through `*` is a `copy`. Class: an aspiration.
- The filtering branch of `INTERSECT` and `EXCEPT` gives a `copy` to each output
  column, since polyglot-sql puts its columns in each projection's upstream as
  `Direct`, where the catalogue of clauses carried over from the proof of concept says
  that the filtering branch decides rows, `rows`. Plan 005 closes it. Class: a defect
  of the code.
- A `USING` key selected bare, the merged key, gives a `derive` from both keys, as
  polyglot-sql reports it, where the catalogue says `copy` from both keys for an inner
  join, and `copy` from the left key with `rows` from the right for a left join. Plan
  005 closes it. Class: a defect of the code.
