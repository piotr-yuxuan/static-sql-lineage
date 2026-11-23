-- The nightly load. GO
/* A block comment
GO
that holds GO on a line of its own */
INSERT INTO dbo.Fact (Id, Amount)
SELECT Id, Amount FROM dbo.Stage
UPDATE dbo.Fact SET Amount = Amount * 2
  DELETE FROM dbo.Fact WHERE Amount < 0
go
PRINT 'GO'
SELECT Id
[GO]
INTO dbo.Gone FROM dbo.Fact
GO 2
MERGE dbo.Dim AS d
USING dbo.Stage AS s ON d.Id = s.Id
WHEN MATCHED THEN UPDATE SET d.Name = s.Name;
GO
