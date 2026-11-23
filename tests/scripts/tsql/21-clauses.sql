CREATE VIEW dbo.vClauses
AS
WITH c AS (SELECT Id, Name FROM dbo.Customer)
SELECT LEFT(c.Name, 2) AS Initials,
	CASE WHEN c.Id > 0 THEN 1 END AS Positive,
	STRING_AGG(o.Note, ',') WITHIN GROUP (ORDER BY o.Note) AS Notes
FROM c
LEFT OUTER JOIN dbo.Orders AS o ON o.CustomerId = c.Id
CROSS APPLY dbo.Lines(o.Id) AS l
WHERE c.Name <> N'FROM'
GROUP BY c.Id, c.Name
HAVING COUNT(*) > 1
GO
MERGE INTO dbo.Dim AS d
USING dbo.Stage AS s ON d.Id = s.Id
WHEN MATCHED THEN UPDATE SET d.Name = s.Name
WHEN NOT MATCHED THEN INSERT (Id, Name) VALUES (s.Id, s.Name);
GO
