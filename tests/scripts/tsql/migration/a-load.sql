SELECT Id, Amount INTO dbo.Fact FROM dbo.Stage
INSERT INTO dbo.Fact (Id, Amount) SELECT Id, Amount FROM dbo.Stage2
UPDATE dbo.Fact SET Amount = Amount * 2
GO
INSERT INTO dbo.Loop1 (Id) SELECT Id FROM dbo.Loop2
INSERT INTO dbo.Loop2 (Id) SELECT Id FROM dbo.Loop1
GO
