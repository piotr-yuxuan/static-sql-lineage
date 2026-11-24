create view v_top as select k from t;
merge into t using s on t.k = s.k when matched then update set * when not matched then insert *;
create view s as select 1 as k;
