CREATE VIEW [dbo].[vCafé]
AS
SELECT [Clé], [Prix] FROM [dbo].[Thé]
GO
  /* é */ INSERT INTO [dbo].[Tarte] ([Clé]) SELECT [Clé] FROM [dbo].[vCafé]
GO
