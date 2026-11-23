INSERT INTO dbo.Fact (Id, Amount) SELECT Id, Amount FROM dbo.Stage
INSERT INTO dbo.Fact (Id, Amount) SELECT Id, Amount * 2 FROM dbo.Stage
INSERT INTO dbo.Fact (Id, Amount) SELECT Id, Amount FROM dbo.Stage2
INSERT INTO dbo.Fact (Nope) SELECT Id FROM dbo.Stage
