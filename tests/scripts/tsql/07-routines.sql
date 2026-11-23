CREATE OR ALTER VIEW [dbo].[vSign]
AS
SELECT o.[Id],
       CASE WHEN o.[Amount] >= 0 THEN N'credit'
ELSE N'debit'
END AS [Sign]
FROM [dbo].[Orders] AS o
GO
ALTER VIEW [dbo].[vLate] AS
SELECT [Id] FROM [dbo].[Orders]
UNION ALL
SELECT [Id] FROM [dbo].[Returns];
GO
CREATE PROCEDURE [dbo].[LoadFact]
AS
BEGIN
    SET NOCOUNT ON;
    INSERT INTO [dbo].[Fact] ([Id])
    SELECT [Id] FROM [dbo].[Stage];
END
GO
