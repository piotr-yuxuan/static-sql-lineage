USE [Sales]
GO
/****** Object:  View [dbo].[vCustomer]    Script Date: 02/01/2026 09:00:00 ******/
IF  EXISTS (SELECT * FROM sys.views WHERE object_id = OBJECT_ID(N'[dbo].[vCustomer]'))
DROP VIEW [dbo].[vCustomer]
GO
/****** Object:  View [dbo].[vCustomer]    Script Date: 02/01/2026 09:00:00 ******/
SET ANSI_NULLS ON
GO
SET QUOTED_IDENTIFIER ON
GO
CREATE VIEW [dbo].[vCustomer]
AS
SELECT c.[Id], c.[Name]
FROM [dbo].[Customer] AS c
WHERE c.[Name] <> N''
GO
