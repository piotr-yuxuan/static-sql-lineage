IF NOT EXISTS (SELECT * FROM sys.views WHERE object_id = OBJECT_ID(N'[dbo].[vQuoted]'))
EXEC dbo.sp_executesql @statement = N'CREATE VIEW [dbo].[vQuoted] AS SELECT o.[Id] FROM [dbo].[Orders] AS o WHERE o.[Status] = ''open'' AND o.[Note] <> ''x'' GROUP BY o.[Id]'
GO
