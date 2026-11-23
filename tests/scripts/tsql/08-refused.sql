UPDATE dbo.StageA SET X = 1
INSERT dbo.StageA (Id, X)
SELECT Id, X FROM dbo.Source
DELETE FROM dbo.StageA WHERE X IS NULL
IF OBJECT_ID(N'dbo.vOld', N'V') IS NOT NULL DROP VIEW dbo.vOld
