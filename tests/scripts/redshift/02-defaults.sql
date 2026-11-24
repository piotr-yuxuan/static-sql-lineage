create view v_top as select id from sales.public.v_base;
create view v_base as select id from orders;
