CREATE TABLE dbo.OrderLine (
    OrderId int NOT NULL,
    Region nvarchar(20) NOT NULL,
    Quantity int NOT NULL,
    UnitPrice decimal(10, 2) NOT NULL,
    Status nvarchar(10) NOT NULL
)
GO
CREATE TABLE dbo.Sales (
    OrderId int NOT NULL,
    Region nvarchar(20) NOT NULL,
    Amount decimal(12, 2) NOT NULL
)
GO
