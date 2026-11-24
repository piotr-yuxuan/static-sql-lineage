create table s (a int, `B` int, c int) using delta;
create function f(x int) returns int language python as $$
return x; select 1;
$$;
create view v as select a, `B` from s where c > 0;
create table w (a int, b int);
insert into w (a, b) select a, `B` from v;
