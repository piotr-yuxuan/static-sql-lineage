use schema SALES.MART;
create view V_TOP as select ID from V_BASE;
create view SALES.MART.V_BASE as select ID from SALES.RAW.ORDERS;
use schema STAGE;
create view V_LOW as select ID from SALES.STAGE.V_SRC;
create view V_SRC as select ID from SALES.RAW.ORDERS;
