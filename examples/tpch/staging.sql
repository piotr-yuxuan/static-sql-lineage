-- One view per table that the marts read, with plain names.

CREATE VIEW stg_line AS
SELECT
    l_orderkey AS order_key,
    l_partkey AS part_key,
    l_suppkey AS supp_key,
    l_quantity AS quantity,
    l_extendedprice * (1 - l_discount) AS net_price,
    l_extendedprice * (1 - l_discount) * (1 + l_tax) AS gross_price,
    l_shipdate AS ship_date,
    l_returnflag = 'R' AS returned
FROM lineitem
WHERE l_linestatus = 'F';

CREATE VIEW stg_order AS
SELECT
    o_orderkey AS order_key,
    o_custkey AS customer_key,
    o_orderdate AS order_date,
    o_orderpriority AS priority
FROM orders
WHERE o_orderstatus <> 'P';

CREATE VIEW stg_customer AS
SELECT
    c.c_custkey AS customer_key,
    c.c_name AS customer,
    c.c_mktsegment AS segment,
    n.n_name AS nation,
    r.r_name AS region
FROM customer AS c
JOIN nation AS n ON n.n_nationkey = c.c_nationkey
JOIN region AS r ON r.r_regionkey = n.n_regionkey;

CREATE VIEW stg_supplier AS
SELECT
    s.s_suppkey AS supp_key,
    s.s_name AS supplier,
    n.n_name AS nation
FROM supplier AS s
JOIN nation AS n ON n.n_nationkey = s.s_nationkey;
