CREATE VIEW region_revenue AS
SELECT region, sum(amount) AS revenue
FROM sales
GROUP BY region;

CREATE VIEW region_orders AS
SELECT region, count(order_id) AS orders
FROM sales
GROUP BY region;
