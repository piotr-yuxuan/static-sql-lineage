CREATE VIEW dbo.vRegionRevenue AS
SELECT Region, SUM(Amount) AS Revenue
FROM dbo.Sales
GROUP BY Region
GO
CREATE VIEW dbo.vRegionOrders AS
SELECT Region, COUNT(OrderId) AS Orders
FROM dbo.Sales
GROUP BY Region
GO
