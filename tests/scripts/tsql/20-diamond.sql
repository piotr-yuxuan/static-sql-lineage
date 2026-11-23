CREATE TABLE dbo.Base (Id int)
GO
CREATE VIEW dbo.vShared AS SELECT Id FROM dbo.Base
GO
CREATE VIEW dbo.vLeft AS SELECT Id FROM dbo.vShared
GO
CREATE VIEW dbo.vRight AS SELECT Id FROM dbo.vShared
GO
CREATE VIEW dbo.vTop AS SELECT l.Id FROM dbo.vLeft AS l JOIN dbo.vRight AS r ON r.Id = l.Id
GO
