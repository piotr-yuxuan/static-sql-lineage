-- Orders, customers and suppliers, each with its figures.

CREATE VIEW order_revenue AS
SELECT
    o.order_key,
    o.customer_key,
    o.order_date,
    sum(l.net_price) AS revenue,
    count(*) AS line_count
FROM stg_order AS o
JOIN stg_line AS l ON l.order_key = o.order_key
WHERE NOT l.returned
GROUP BY o.order_key, o.customer_key, o.order_date;

CREATE VIEW customer_revenue AS
SELECT
    c.customer_key,
    c.customer,
    c.nation,
    c.region,
    sum(o.revenue) AS revenue,
    count(o.order_key) AS orders
FROM stg_customer AS c
JOIN order_revenue AS o ON o.customer_key = c.customer_key
GROUP BY c.customer_key, c.customer, c.nation, c.region;

CREATE VIEW supplier_margin AS
SELECT
    s.supplier,
    s.nation,
    sum(l.net_price) - sum(ps.ps_supplycost * l.quantity) AS margin
FROM stg_line AS l
JOIN partsupp AS ps ON ps.ps_partkey = l.part_key AND ps.ps_suppkey = l.supp_key
JOIN stg_supplier AS s ON s.supp_key = l.supp_key
GROUP BY s.supplier, s.nation;

CREATE VIEW nation_party AS
SELECT nation, customer AS party, 'customer' AS kind
FROM stg_customer
UNION ALL
SELECT nation, supplier, 'supplier'
FROM stg_supplier;
