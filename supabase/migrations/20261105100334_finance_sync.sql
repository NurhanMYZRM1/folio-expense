-- Folio optional personal-ledger sync. Business documents and receipt attachments are not
-- accepted by this first protocol version. No financial data is public.
create schema if not exists folio_private;
create schema if not exists extensions;
create extension if not exists pgcrypto with schema extensions;
revoke all on schema folio_private from public, anon;
grant usage on schema folio_private to authenticated;

create table public.cloud_workspaces (
 id uuid primary key, owner_id uuid not null references auth.users(id) on delete cascade,
 kind text not null check(kind in ('personal','business')), name text not null check(length(name) between 1 and 120),
 created_at timestamptz not null default now()
);
create unique index cloud_one_personal on public.cloud_workspaces(owner_id) where kind='personal';
create table public.workspace_members (
 workspace_id uuid not null references public.cloud_workspaces(id) on delete cascade,
 user_id uuid not null references auth.users(id) on delete cascade,
 role text not null check(role in ('owner','admin','editor','viewer')),
 primary key(workspace_id,user_id)
);
create table public.sync_records (
 workspace_id uuid not null references public.cloud_workspaces(id) on delete cascade,
 entity text not null check(entity in ('workspaces','categories','accounts','transactions','budgets','bills')),
 id uuid not null, version bigint not null check(version>0), data jsonb not null,
 deleted_at timestamptz, server_seq bigint generated always as identity,
 updated_by uuid not null references auth.users(id), updated_at timestamptz not null default now(),
 primary key(workspace_id,entity,id), unique(server_seq)
);
create index sync_records_pull on public.sync_records(workspace_id,server_seq);
create table folio_private.applied_changes (
 user_id uuid not null references auth.users(id) on delete cascade, change_id uuid not null,
 workspace_id uuid not null references public.cloud_workspaces(id) on delete cascade,
 payload_hash bytea not null, result jsonb not null,
 primary key(user_id,change_id)
);
create table folio_private.invitations (
 code_hash bytea primary key, workspace_id uuid not null references public.cloud_workspaces(id) on delete cascade,
 role text not null check(role in ('admin','editor','viewer')), created_by uuid not null references auth.users(id),
 expires_at timestamptz not null default now()+interval '7 days', revoked_at timestamptz, redeemed_by uuid references auth.users(id), redeemed_at timestamptz
);
create table folio_private.join_attempts (user_id uuid primary key references auth.users(id) on delete cascade, window_start timestamptz not null, attempts integer not null);

alter table public.cloud_workspaces enable row level security;
alter table public.workspace_members enable row level security;
alter table public.sync_records enable row level security;
alter table folio_private.applied_changes enable row level security;
alter table folio_private.invitations enable row level security;
alter table folio_private.join_attempts enable row level security;
revoke all on public.cloud_workspaces,public.workspace_members,public.sync_records from public,anon,authenticated;
revoke all on all tables in schema folio_private from public,anon,authenticated;
grant select on public.cloud_workspaces,public.workspace_members,public.sync_records to authenticated;

-- This internal lookup avoids recursive membership policies; callers cannot choose a user id.
create function folio_private.member_role(w uuid) returns text language sql stable security definer set search_path=''
as $$select role from public.workspace_members where workspace_id=w and user_id=auth.uid()$$;
create policy member_workspace on public.cloud_workspaces for select to authenticated using(folio_private.member_role(id) is not null);
create policy member_members on public.workspace_members for select to authenticated using(folio_private.member_role(workspace_id) is not null);
create policy member_records on public.sync_records for select to authenticated using(folio_private.member_role(workspace_id) is not null);

create function folio_private.create_workspace(w uuid, workspace_kind text, workspace_name text) returns jsonb
language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); existing public.cloud_workspaces;
begin
 if actor is null then raise exception 'Authentication required' using errcode='42501'; end if;
 select * into existing from public.cloud_workspaces where id=w for update;
 if found then
  if existing.owner_id<>actor then raise exception 'Workspace unavailable' using errcode='42501'; end if;
  return jsonb_build_object('id',w,'kind',existing.kind,'name',existing.name);
 end if;
 insert into public.cloud_workspaces(id,owner_id,kind,name) values(w,actor,workspace_kind,workspace_name);
 insert into public.workspace_members values(w,actor,'owner');
 return jsonb_build_object('id',w,'kind',workspace_kind,'name',workspace_name);
end$$;
create function public.create_cloud_workspace(w uuid, workspace_kind text, workspace_name text) returns jsonb language sql security invoker set search_path='' as $$select folio_private.create_workspace(w,workspace_kind,workspace_name)$$;

create function folio_private.invite(w uuid, invited_role text) returns jsonb language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); actor_role text; code text:=''; bytes bytea; i integer;
begin
 perform 1 from public.cloud_workspaces where id=w for update;
 actor_role:=folio_private.member_role(w);
 if actor is null or actor_role not in ('owner','admin') or invited_role not in ('admin','editor','viewer') or (invited_role='admin' and actor_role<>'owner') then raise exception 'Forbidden' using errcode='42501'; end if;
 if (select kind from public.cloud_workspaces where id=w)<>'business' then raise exception 'Personal workspaces cannot be shared'; end if;
 bytes:=extensions.gen_random_bytes(12);
 for i in 0..11 loop code:=code||substr('0123456789ABCDEFGHJKMNPQRSTVWXYZ',1+(get_byte(bytes,i)&31),1); end loop;
 insert into folio_private.invitations(code_hash,workspace_id,role,created_by) values(extensions.digest(code,'sha256'),w,invited_role,actor);
 return jsonb_build_object('code',code,'expiresAt',now()+interval '7 days');
end$$;
create function public.invite_workspace_member(w uuid, invited_role text) returns jsonb language sql security invoker set search_path='' as $$select folio_private.invite(w,invited_role)$$;
create function folio_private.join_workspace(code text) returns jsonb language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); n integer; invite folio_private.invitations;
begin
 if actor is null then raise exception 'Authentication required' using errcode='42501'; end if;
 insert into folio_private.join_attempts values(actor,now(),1) on conflict(user_id) do update set
 attempts=case when join_attempts.window_start<now()-interval '1 hour' then 1 else join_attempts.attempts+1 end,
 window_start=case when join_attempts.window_start<now()-interval '1 hour' then now() else join_attempts.window_start end returning attempts into n;
 if n>10 then return jsonb_build_object('error','rate_limited'); end if;
 select * into invite from folio_private.invitations where code_hash=extensions.digest(upper(replace(code,'-','')),'sha256') for update;
 if not found or invite.revoked_at is not null or invite.redeemed_at is not null or invite.expires_at<=now() then return jsonb_build_object('error','invalid_invitation'); end if;
 perform 1 from public.cloud_workspaces where id=invite.workspace_id for update;
 if (select kind from public.cloud_workspaces where id=invite.workspace_id)<>'business' then return jsonb_build_object('error','invalid_invitation'); end if;
 -- Joining never changes an existing member's role or demotes the owner.
 insert into public.workspace_members values(invite.workspace_id,actor,invite.role) on conflict do nothing;
 update folio_private.invitations set redeemed_by=actor,redeemed_at=now() where code_hash=invite.code_hash;
 return jsonb_build_object('workspaceId',invite.workspace_id,'role',folio_private.member_role(invite.workspace_id));
end$$;
create function public.join_workspace(code text) returns jsonb language sql security invoker set search_path='' as $$select folio_private.join_workspace(code)$$;
create function folio_private.revoke_invite(w uuid,code text) returns void language plpgsql security definer set search_path='' as $$
begin
 perform 1 from public.cloud_workspaces where id=w for update;
 if auth.uid() is null or coalesce(folio_private.member_role(w),'') not in ('owner','admin') then raise exception 'Forbidden' using errcode='42501'; end if;
 update folio_private.invitations set revoked_at=now() where workspace_id=w and code_hash=extensions.digest(upper(replace(code,'-','')),'sha256');
end$$;
create function public.revoke_workspace_invitation(w uuid,code text) returns void language sql security invoker set search_path='' as $$select folio_private.revoke_invite(w,code)$$;
create function folio_private.remove_member(w uuid,member uuid) returns void language plpgsql security definer set search_path='' as $$
declare actor_role text; target_role text;
begin
 perform 1 from public.cloud_workspaces where id=w for update;
 actor_role:=folio_private.member_role(w);
 select role into target_role from public.workspace_members where workspace_id=w and user_id=member;
 if auth.uid() is null or actor_role is null or actor_role not in ('owner','admin') or target_role='owner' or (actor_role='admin' and target_role='admin') then raise exception 'Forbidden' using errcode='42501'; end if;
 delete from public.workspace_members where workspace_id=w and user_id=member;
end$$;
create function public.remove_workspace_member(w uuid,member uuid) returns void language sql security invoker set search_path='' as $$select folio_private.remove_member(w,member)$$;

-- Validate financial invariants with NUMERIC, never binary floating point. Client-derived
-- display fields (balance/category names) are absent from row payloads.
create function folio_private.validate_graph(w uuid) returns void language plpgsql security definer set search_path='' as $$
declare r public.sync_records; d jsonb; a jsonb; b jsonb; c jsonb; balance numeric; required text[]; field text; pair record;
begin
 for r in select * from public.sync_records where workspace_id=w and deleted_at is null loop
  d:=r.data;
  required:=case r.entity
   when 'workspaces' then array['id','kind','name','base_currency','version','created_at','updated_at','sync_state']
   when 'categories' then array['id','workspace_id','kind','name','version','created_at','updated_at','sync_state']
   when 'accounts' then array['id','workspace_id','kind','name','currency','opening_balance_minor','note','version','created_at','updated_at','sync_state']
   when 'transactions' then array['id','workspace_id','kind','account_id','amount_minor','occurred_on','note','source','version','created_at','updated_at','sync_state']
   when 'budgets' then array['id','workspace_id','amount_minor','alert_percent','version','created_at','updated_at','sync_state']
   when 'bills' then array['id','workspace_id','kind','name','amount_minor','account_id','note','frequency','interval_count','anchor_on','remind_days_before','reminders_enabled','version','created_at','updated_at','sync_state'] end;
  foreach field in array required loop if d->>field is null then raise exception 'Missing required field'; end if; end loop;
  for pair in select key,value from jsonb_each(d) loop
   if pair.key like '%_minor' and pair.value<>'null'::jsonb and (jsonb_typeof(pair.value)<>'number' or pair.value::text !~ '^-?[0-9]+$') then raise exception 'Money must use integer minor units'; end if;
  end loop;
  if d->>'id' is distinct from r.id::text or (r.entity<>'workspaces' and d->>'workspace_id' is distinct from w::text) then raise exception 'Invalid record scope'; end if;
  if r.entity='workspaces' then
   if r.id<>w or d->>'kind'<>(select kind from public.cloud_workspaces where id=w) then raise exception 'Invalid workspace'; end if;
  elsif r.entity='categories' then
   if d->>'kind' not in ('income','expense') or length(d->>'name') not between 1 and 60 then raise exception 'Invalid category'; end if;
  elsif r.entity='accounts' then
   if d->>'kind' not in ('cash','bank','ewallet','card','savings','loan','other') or d->>'currency' !~ '^[A-Z]{3}$' or length(d->>'name') not between 1 and 120 or abs((d->>'opening_balance_minor')::numeric)>999999999999 then raise exception 'Invalid account'; end if;
   select (d->>'opening_balance_minor')::numeric+coalesce(sum(case when t.data->>'account_id'=r.id::text then case when t.data->>'kind'='income' then (t.data->>'amount_minor')::numeric else -(t.data->>'amount_minor')::numeric end else (t.data->>'to_amount_minor')::numeric end),0) into balance from public.sync_records t where t.workspace_id=w and t.entity='transactions' and t.deleted_at is null and (t.data->>'account_id'=r.id::text or t.data->>'to_account_id'=r.id::text);
   if abs(balance)>9007199254740991 or (d->>'archived_at' is not null and balance<>0) or (d->>'loan_direction'='borrowed' and balance>0) or (d->>'loan_direction'='lent' and balance<0) then raise exception 'Invalid account balance'; end if;
  elsif r.entity='transactions' then
   if d->>'kind' not in ('income','expense','transfer') or (d->>'amount_minor')::numeric not between 1 and 999999999999 or d->>'occurred_on' !~ '^\d{4}-\d{2}-\d{2}$' then raise exception 'Invalid transaction'; end if;
   perform (d->>'occurred_on')::date;
   if d->>'source' not in ('manual','quick_add','voice','notification','bill') or d->>'expense_id' is not null then raise exception 'Source requires trusted document validation'; end if;
   select data into a from public.sync_records where workspace_id=w and entity='accounts' and id=(d->>'account_id')::uuid and deleted_at is null;
   if a is null then raise exception 'Account not in workspace'; end if;
   if d->>'kind'='transfer' then
    select data into b from public.sync_records where workspace_id=w and entity='accounts' and id=(d->>'to_account_id')::uuid and deleted_at is null;
    if b is null or d->>'to_account_id'=d->>'account_id' or d->>'category_id' is not null or (d->>'to_amount_minor')::numeric not between 1 and 999999999999 or (a->>'currency'=b->>'currency' and d->>'amount_minor'<>d->>'to_amount_minor') then raise exception 'Invalid transfer'; end if;
   elsif d->>'to_account_id' is not null or d->>'to_amount_minor' is not null then raise exception 'Invalid transfer fields'; end if;
   if d->>'category_id' is not null then
    select data into c from public.sync_records where workspace_id=w and entity='categories' and id=(d->>'category_id')::uuid and deleted_at is null;
    if c is null or c->>'kind'<>d->>'kind' then raise exception 'Category not in workspace or kind'; end if;
   end if;
   if (d->>'bill_id' is null)<>(d->>'bill_due_on' is null) then raise exception 'Invalid bill fields'; end if;
   if d->>'bill_id' is not null then
    select data into b from public.sync_records where workspace_id=w and entity='bills' and id=(d->>'bill_id')::uuid and deleted_at is null;
    if b is null or d->>'kind'<>'expense' or d->>'source'<>'bill' then raise exception 'Invalid bill payment'; end if;
    perform (d->>'bill_due_on')::date;
   end if;
  elsif r.entity in ('budgets','bills') then
   if (d->>'amount_minor')::numeric not between 1 and 999999999999 then raise exception 'Invalid scheduled amount'; end if;
   if d->>'category_id' is not null then
    select data into c from public.sync_records where workspace_id=w and entity='categories' and id=(d->>'category_id')::uuid and deleted_at is null;
    if c is null or c->>'kind'<>'expense' then raise exception 'Invalid expense category'; end if;
   end if;
   if r.entity='budgets' and (d->>'alert_percent')::integer not between 1 and 100 then raise exception 'Invalid budget threshold'; end if;
   if r.entity='bills' then
    select data into a from public.sync_records where workspace_id=w and entity='accounts' and id=(d->>'account_id')::uuid and deleted_at is null;
    if a is null or d->>'frequency' not in ('once','weekly','monthly','yearly') or (d->>'interval_count')::integer not between 1 and 52 then raise exception 'Invalid bill'; end if;
    perform (d->>'anchor_on')::date;
   end if;
  end if;
 end loop;
 if exists(select 1 from public.sync_records where workspace_id=w and entity='transactions' and deleted_at is null and data->>'bill_id' is not null group by data->>'bill_id',data->>'bill_due_on' having count(*)>1) then raise exception 'Duplicate bill payment'; end if;
 if exists(select 1 from public.sync_records where workspace_id=w and entity='budgets' and deleted_at is null and data->>'archived_at' is null group by coalesce(data->>'category_id','') having count(*)>1) then raise exception 'Duplicate budget'; end if;
 if exists(select 1 from public.sync_records where workspace_id=w and entity='categories' and deleted_at is null group by data->>'kind',lower(data->>'name') having count(*)>1) then raise exception 'Duplicate category'; end if;
end$$;

create function folio_private.sync_push(w uuid,changes jsonb) returns jsonb language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); actor_role text; ch jsonb; existing public.sync_records; saved folio_private.applied_changes; cid uuid; rid uuid; ent text; result jsonb; results jsonb:='[]'; hashed bytea;
begin
 perform 1 from public.cloud_workspaces where id=w for update;
 actor_role:=folio_private.member_role(w);
 if actor is null or actor_role is null or actor_role not in ('owner','admin','editor') then raise exception 'Forbidden' using errcode='42501'; end if;
 if jsonb_typeof(changes)<>'array' or jsonb_array_length(changes)>500 then raise exception 'Invalid batch'; end if;
 -- Exception subtransaction makes a graph-invalid batch entirely atomic.
 begin
 for ch in select value from jsonb_array_elements(changes) loop
  cid:=(ch->>'changeId')::uuid; rid:=(ch->>'id')::uuid; ent:=ch->>'entity'; hashed:=extensions.digest(ch::text,'sha256');
  if ch->>'workspaceId' is distinct from w::text or ent not in ('workspaces','categories','accounts','transactions','budgets','bills') or ch->>'operation' not in ('upsert','delete') then raise exception 'Invalid change scope'; end if;
  if ent='workspaces' and (actor_role<>'owner' or rid<>w or ch->>'operation'='delete') then raise exception 'Workspace ownership required'; end if;
  select * into saved from folio_private.applied_changes where user_id=actor and change_id=cid;
  if found then
   if saved.payload_hash<>hashed or saved.workspace_id<>w then raise exception 'Change id reused'; end if;
   results:=results||jsonb_build_array(saved.result); continue;
  end if;
  select * into existing from public.sync_records where workspace_id=w and entity=ent and id=rid;
  if existing.deleted_at is not null then
   result:=jsonb_build_object('changeId',cid,'status',case when ch->>'operation'='delete' then 'applied' else 'rejected' end,'record',to_jsonb(existing));
  elsif existing.id is not null and existing.version<>(ch->>'baseVersion')::bigint and ch->>'operation'<>'delete' then
   result:=jsonb_build_object('changeId',cid,'status','conflict','record',to_jsonb(existing));
  elsif existing.id is null and (ch->>'baseVersion')::bigint<>0 and ch->>'operation'<>'delete' then
   result:=jsonb_build_object('changeId',cid,'status','rejected','reason','missing_base');
  else
   if ch->>'operation'='upsert' and (jsonb_typeof(ch->'data')<>'object' or octet_length((ch->'data')::text)>200000) then raise exception 'Invalid payload'; end if;
   insert into public.sync_records(workspace_id,entity,id,version,data,deleted_at,updated_by)
   values(w,ent,rid,coalesce(existing.version,0)+1,case when ch->>'operation'='delete' then '{}' else ch->'data' end,case when ch->>'operation'='delete' then now() else null end,actor)
   on conflict(workspace_id,entity,id) do update set version=excluded.version,data=excluded.data,deleted_at=excluded.deleted_at,server_seq=default,updated_by=actor,updated_at=now() returning * into existing;
   result:=jsonb_build_object('changeId',cid,'status','applied','record',to_jsonb(existing));
  end if;
  insert into folio_private.applied_changes values(actor,cid,w,hashed,result);
  results:=results||jsonb_build_array(result);
 end loop;
 perform folio_private.validate_graph(w);
 exception when check_violation or invalid_text_representation or raise_exception or not_null_violation or numeric_value_out_of_range or datetime_field_overflow then
  return (select coalesce(jsonb_agg(jsonb_build_object('changeId',value->>'changeId','status','rejected','reason','invalid_financial_graph')),'[]') from jsonb_array_elements(changes));
 end;
 return results;
end$$;
create function public.sync_push(w uuid,changes jsonb) returns jsonb language sql security invoker set search_path='' as $$select folio_private.sync_push(w,changes)$$;
create function folio_private.sync_pull(w uuid,since bigint,page_limit integer default 200) returns jsonb language plpgsql security definer set search_path='' as $$
begin
 if auth.uid() is null or folio_private.member_role(w) is null then raise exception 'Forbidden' using errcode='42501'; end if;
 if since<0 or page_limit not between 1 and 500 then raise exception 'Invalid cursor'; end if;
 return (select coalesce(jsonb_agg(to_jsonb(r) order by r.server_seq),'[]') from (select * from public.sync_records where workspace_id=w and server_seq>since order by server_seq limit page_limit) r);
end$$;
create function public.sync_pull(w uuid,since bigint,page_limit integer default 200) returns jsonb language sql security invoker set search_path='' as $$select folio_private.sync_pull(w,since,page_limit)$$;

-- Narrow callable API, including explicit revocation of PostgreSQL's default PUBLIC EXECUTE.
revoke all on all functions in schema folio_private from public,anon,authenticated;
grant execute on function folio_private.member_role(uuid),folio_private.create_workspace(uuid,text,text),folio_private.invite(uuid,text),folio_private.join_workspace(text),folio_private.revoke_invite(uuid,text),folio_private.remove_member(uuid,uuid),folio_private.sync_push(uuid,jsonb),folio_private.sync_pull(uuid,bigint,integer) to authenticated;
revoke all on function public.create_cloud_workspace(uuid,text,text),public.invite_workspace_member(uuid,text),public.join_workspace(text),public.revoke_workspace_invitation(uuid,text),public.remove_workspace_member(uuid,uuid),public.sync_push(uuid,jsonb),public.sync_pull(uuid,bigint,integer) from public,anon,authenticated;
grant execute on function public.create_cloud_workspace(uuid,text,text),public.invite_workspace_member(uuid,text),public.join_workspace(text),public.revoke_workspace_invitation(uuid,text),public.remove_workspace_member(uuid,uuid),public.sync_push(uuid,jsonb),public.sync_pull(uuid,bigint,integer) to authenticated;

-- Private Storage rules. Receipt upload/download engine is a later slice; this bucket
-- cannot be read anonymously and paths cannot move between workspaces.
insert into storage.buckets(id,name,public,file_size_limit) values('folio-receipts','folio-receipts',false,26214400) on conflict(id) do nothing;
create policy folio_receipt_read on storage.objects for select to authenticated using(bucket_id='folio-receipts' and folio_private.member_role(((storage.foldername(name))[1])::uuid) is not null);
create policy folio_receipt_insert on storage.objects for insert to authenticated with check(bucket_id='folio-receipts' and folio_private.member_role(((storage.foldername(name))[1])::uuid) in ('owner','admin','editor'));
create policy folio_receipt_update on storage.objects for update to authenticated using(bucket_id='folio-receipts' and folio_private.member_role(((storage.foldername(name))[1])::uuid) in ('owner','admin','editor')) with check(bucket_id='folio-receipts' and folio_private.member_role(((storage.foldername(name))[1])::uuid) in ('owner','admin','editor'));
create policy folio_receipt_delete on storage.objects for delete to authenticated using(bucket_id='folio-receipts' and folio_private.member_role(((storage.foldername(name))[1])::uuid) in ('owner','admin','editor'));
