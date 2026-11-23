create view v_top as select id from sales.main.v_base;
create view v_base as select id from orders;
