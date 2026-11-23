-- What the business reads.

CREATE VIEW nation_revenue AS
SELECT region, nation, sum(revenue) AS revenue, sum(orders) AS orders
FROM customer_revenue
GROUP BY region, nation;

CREATE VIEW region_report AS
SELECT region, sum(revenue) AS revenue, sum(revenue) / sum(orders) AS average_order
FROM nation_revenue
GROUP BY region;

CREATE VIEW top_customers AS
SELECT
    region,
    customer,
    revenue,
    rank() OVER (PARTITION BY region ORDER BY revenue DESC) AS position
FROM customer_revenue
QUALIFY rank() OVER (PARTITION BY region ORDER BY revenue DESC) <= 10;
