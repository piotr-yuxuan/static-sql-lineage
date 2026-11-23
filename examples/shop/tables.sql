CREATE TABLE order_line (
    order_id INTEGER NOT NULL,
    region VARCHAR NOT NULL,
    quantity INTEGER NOT NULL,
    unit_price DECIMAL(10, 2) NOT NULL,
    status VARCHAR NOT NULL
);

CREATE TABLE sales (
    order_id INTEGER NOT NULL,
    region VARCHAR NOT NULL,
    amount DECIMAL(12, 2) NOT NULL
);
