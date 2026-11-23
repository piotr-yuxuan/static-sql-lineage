INSERT INTO sales (order_id, region, amount)
SELECT order_id, region, quantity * unit_price
FROM order_line
WHERE status = 'shipped';
