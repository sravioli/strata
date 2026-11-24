-- The search of the API reads the text parts of its tenant (tech design 0049,
-- change 4). strata_owner applies this file (`strata migrate`).

-- The grants of the spec tenant-isolation. An L1 test reads them from the
-- catalog.
grant select on text_parts to strata_api;
