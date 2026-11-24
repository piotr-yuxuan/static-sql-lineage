create table s (a int, "B" int, c int);
create function f() returns int language plpgsql as $body$
begin
  insert into s (a) select 1;
  return 1;
end
$body$;
do $$ begin perform 1; end $$;
set search_path = analytics, public;
create view v as select a, "B" from s where c > 0;
create table w (a int, b int);
insert into w (a, b) select a, "B" from v;
