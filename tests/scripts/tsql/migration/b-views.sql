CREATE VIEW dbo.vB AS SELECT Id FROM dbo.vA
GO
CREATE VIEW [dbo].[vC] AS SELECT f.Id FROM dbo.Fact AS f JOIN dbo.vB AS b ON b.Id = f.Id
GO
