CREATE VIEW dbo.vFact AS SELECT Id FROM dbo.Fact
GO
SELECT Id, Amount INTO dbo.Fact FROM dbo.Stage
UPDATE dbo.Fact SET Amount = Amount * 2
INSERT INTO dbo.Fact (Id, Amount) SELECT Id, Amount FROM dbo.Stage2
GO
