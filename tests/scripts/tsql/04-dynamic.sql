EXEC ('CREATE SCHEMA [stage]')
GO
DECLARE @sql nvarchar(max) = N'SELECT 1 AS a'
EXEC (@sql)
EXEC sp_executesql @sql
EXECUTE dbo.sp_executesql @statement = N'CREATE VIEW [dbo].[vHalf] AS'
GO
EXEC dbo.LoadOrders @Day = N'2026-01-02'
GO
