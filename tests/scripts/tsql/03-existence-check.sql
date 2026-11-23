USE [Sales]
GO
/****** Object:  View [dbo].[vOpen]    Script Date: 02/01/2026 09:00:00 ******/
SET ANSI_NULLS ON
GO
SET QUOTED_IDENTIFIER ON
GO
IF NOT EXISTS (SELECT * FROM sys.views WHERE object_id = OBJECT_ID(N'[dbo].[vOpen]'))
EXEC dbo.sp_executesql @statement = N'CREATE VIEW [dbo].[vOpen]
AS
SELECT o.[Id], o.[Amount]
FROM [dbo].[Orders] AS o
WHERE o.[Status] = ''open''
'
GO
