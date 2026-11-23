USE [Mart]
GO
CREATE VIEW dbo.vTop AS SELECT Id FROM Sales.dbo.vBase
GO
CREATE VIEW dbo.vBare AS SELECT Id FROM Sales.dbo.vPlain
GO
USE [Sales]
GO
CREATE VIEW dbo.vBase AS SELECT Id FROM dbo.Orders
GO
CREATE VIEW vPlain AS SELECT Id FROM dbo.Orders
GO
