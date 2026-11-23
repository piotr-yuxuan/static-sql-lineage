CREATE TABLE dbo.Loop1 (Id int)
CREATE TABLE dbo.Loop2 (Id int)
INSERT INTO dbo.Loop1 (Id) SELECT Id FROM dbo.Loop2
INSERT INTO dbo.Loop2 (Id) SELECT Id FROM dbo.Loop1
