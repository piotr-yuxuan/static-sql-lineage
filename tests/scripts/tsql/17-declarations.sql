CREATE TABLE dbo.Empty ()
GO
CREATE TABLE dbo.Twice (Id int, ID int)
GO
CREATE TABLE dbo.Stage (Id int, Amount int)
GO
CREATE TABLE [dbo].[Stage] ([Id] int, [Amount] int, [Note] nvarchar(10))
GO
CREATE VIEW dbo.vStage AS SELECT * FROM dbo.Stage
GO
