CREATE TABLE [dbo].[Doc](
	[Id] [uniqueidentifier] ROWGUIDCOL NOT NULL,
	[Body] [nvarchar](max) NULL,
	[Blob] [varbinary](max) FILESTREAM NULL,
	[Note] [nvarchar](10) SPARSE NULL,
	sparse int NULL
) ON [PRIMARY] TEXTIMAGE_ON [PRIMARY] FILESTREAM_ON [FS] -- ON [PRIMARY] in a comment
WITH (DATA_COMPRESSION = PAGE)
GO
CREATE TABLE dbo.Part (Id int, Day date) ON ps_Day([Day]) TEXTIMAGE_ON [PRIMARY]
GO
CREATE TABLE dbo.Joined (Id) AS SELECT s.Id FROM dbo.Stage AS s JOIN dbo.Other AS o ON o.Id = s.Id
GO
CREATE TABLE dbo.Spread WITH (DISTRIBUTION = ROUND_ROBIN) AS SELECT s.Id FROM dbo.Stage AS s JOIN dbo.Other AS o ON o.Id = s.Id
GO
CREATE VIEW dbo.vDoc AS SELECT * FROM [dbo].[Doc]
GO
