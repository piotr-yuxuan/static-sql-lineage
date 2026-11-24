create table s (a int, "B" int, c int) diststyle key distkey (a) compound sortkey (a, c);
create or replace procedure p() as $$
begin
  insert into s (a) select 1;
end;
$$ language plpgsql;
set search_path to analytics, public;
create view v as select a, "B" from s where c > 0 with no schema binding;
create table w (a int, b int);
insert into w (a, b) select a, "B" from v;
