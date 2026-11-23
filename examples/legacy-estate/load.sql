INSERT INTO dbo.Sales (OrderId, Region, Amount)
SELECT OrderId, Region, Quantity * UnitPrice
FROM dbo.OrderLine
WHERE Status = 'shipped'
GO
