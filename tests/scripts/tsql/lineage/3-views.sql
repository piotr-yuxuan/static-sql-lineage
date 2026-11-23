CREATE VIEW dbo.vTop AS SELECT Total FROM dbo.vBase
GO
CREATE VIEW dbo.vBase AS SELECT id, amount AS Total FROM DBO.FACT
GO
CREATE VIEW dbo.vOther AS SELECT Id FROM dbo.Missing
GO
CREATE VIEW dbo.vBoth AS SELECT b.Total, o.Id FROM dbo.vBase AS b JOIN dbo.vOther AS o ON o.Id = b.id
GO
