-- Integrity rules over action/action_insight (depends on 20260519000100):
--   1. every `action` must cite at least one `insight`
--   2. an action's last `action_insight` link cannot be deleted
-- Both are deferrable-initially-deferred constraint triggers so an action and
-- its first insight link can be inserted in the same transaction.

create or replace function check_action_has_insight()
returns trigger
language plpgsql
as $body$
begin
  if not exists (
    select 1 from action_insight where action_id = new.id
  ) then
    raise exception 'action % must cite at least one insight', new.id
      using errcode = 'check_violation';
  end if;
  return null;
end;
$body$;

create constraint trigger action_must_cite_insight
after insert on action
deferrable initially deferred
for each row
execute function check_action_has_insight();

create or replace function check_action_insight_not_last()
returns trigger
language plpgsql
as $body$
begin
  if not exists (select 1 from action where id = old.action_id) then
    return old;
  end if;

  if not exists (
    select 1 from action_insight
    where action_id = old.action_id
      and insight_id <> old.insight_id
  ) then
    raise exception 'cannot delete last insight link for action %', old.action_id
      using errcode = 'check_violation';
  end if;

  return old;
end;
$body$;

create constraint trigger action_insight_keep_at_least_one
after delete on action_insight
deferrable initially deferred
for each row
execute function check_action_insight_not_last();
