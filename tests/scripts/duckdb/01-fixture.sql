create table raw_orders (id integer, amount integer);
insert into raw_orders values (1, 100), (2, 250);
create view stg_orders as select id, amount / 100 as amount from raw_orders where amount > 0;
create table orders as select o.id, o.amount from stg_orders o;
use memory.main;
