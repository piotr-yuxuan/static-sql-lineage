-- Ported from SQL Server; a ';' in a comment splits nothing.
create or replace view V_ORDERS as
select ID, 'a;b' as NOTE from RAW.ORDERS;
create or replace procedure LOAD() returns varchar language sql as
$$
begin
    insert into FACT select ID from V_ORDERS;
    return 'done';
end
$$;
insert into FACT (ID) select ID from V_ORDERS where ID > 0;;
create table T (A int with masking policy P);
update FACT set ID = ID + 1;
