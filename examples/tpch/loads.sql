-- A nightly load: each day's revenue by region.

INSERT INTO revenue_by_day (ship_date, region, revenue)
SELECT l.ship_date, c.region, sum(l.net_price)
FROM stg_line AS l
JOIN stg_order AS o ON o.order_key = l.order_key
JOIN stg_customer AS c ON c.customer_key = o.customer_key
GROUP BY l.ship_date, c.region;
