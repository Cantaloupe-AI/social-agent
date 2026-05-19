-- Action core + action↔insight join.
-- Prerequisites (assumed already migrated): `analyst`, `insight` tables and the
-- `moddatetime` extension pattern used by their updated_at triggers.

create extension if not exists moddatetime schema extensions;

create table action (
  id          uuid        primary key default gen_random_uuid(),
  body        text        not null,                            -- what to do
  reasoning   text        not null,                            -- why (mirrors insight shape)
  analyst_id  uuid        not null references analyst(id) on delete restrict,
  created_at  timestamptz not null default now(),
  updated_at  timestamptz not null default now()
);

create index action_analyst_id_idx on action (analyst_id);

create trigger action_handle_updated_at
before update on action
for each row execute procedure moddatetime(updated_at);

create table action_insight (
  action_id   uuid        not null references action(id)  on delete cascade,
  insight_id  uuid        not null references insight(id) on delete restrict,
  created_at  timestamptz not null default now(),
  primary key (action_id, insight_id)
);

create index action_insight_insight_id_idx on action_insight (insight_id);
