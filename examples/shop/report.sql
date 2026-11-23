CREATE VIEW region_report AS
SELECT r.region, r.revenue, r.revenue / o.orders AS average_order
FROM region_revenue AS r
JOIN region_orders AS o ON o.region = r.region;
