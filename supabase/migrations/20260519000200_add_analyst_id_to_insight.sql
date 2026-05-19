-- Add the owning analyst to `insight`.
-- NOTE: `set not null` will fail if `insight` already contains rows (existing
-- rows would have a null analyst_id). If `insight` is non-empty, backfill
-- analyst_id between the `add column` and `set not null` statements.

alter table insight
  add column analyst_id uuid references analyst(id) on delete restrict;

alter table insight alter column analyst_id set not null;

create index insight_analyst_id_idx on insight (analyst_id);
