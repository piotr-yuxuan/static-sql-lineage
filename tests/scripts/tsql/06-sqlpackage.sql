:setvar DatabaseName "Sales"
:on error exit
USE [$(DatabaseName)]
GO
PRINT N'Creating View [dbo].[vA]...'
GO
CREATE VIEW [dbo].[vA]
AS
SELECT [Id] FROM [dbo].[Orders]
:setvar Other "x"
CREATE VIEW [dbo].[vB]
AS
SELECT [Id] FROM [dbo].[vA]
GO
