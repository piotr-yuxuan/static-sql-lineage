CREATE VIEW dbo.vRegionReport AS
SELECT r.Region, r.Revenue, r.Revenue / o.Orders AS AverageOrder
FROM dbo.vRegionRevenue AS r
JOIN dbo.vRegionOrders AS o ON o.Region = r.Region
GO
