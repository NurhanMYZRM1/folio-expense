-- Apply after the imported v7 schema. This is the hosted full-domain protocol,
-- independent of the earlier, undeployed personal-ledger-only draft.
create schema if not exists folio_sync;
revoke all on schema folio_sync from public, anon, authenticated;
create table folio_sync.entities(entity text primary key,key_columns text[] not null,data_columns text[] not null);
create table folio_sync.clocks(owner_id uuid primary key references auth.users(id) on delete cascade,seq bigint not null default 0);
create table folio_sync.records(
 owner_id uuid not null references auth.users(id) on delete cascade,
 entity text not null references folio_sync.entities(entity),key jsonb not null,
 revision bigint not null,seq bigint not null,data jsonb,deleted boolean not null,
 primary key(owner_id,entity,key)
);
create table folio_sync.changes(
 owner_id uuid not null references auth.users(id) on delete cascade,seq bigint not null,
 entity text not null,key jsonb not null,revision bigint not null,data jsonb,deleted boolean not null,batch text not null default txid_current()::text,
 primary key(owner_id,seq)
);
create table folio_sync.operations(
 owner_id uuid not null references auth.users(id) on delete cascade,change_id uuid not null,
 fingerprint text not null,result jsonb not null,primary key(owner_id,change_id)
);
alter table folio_sync.entities enable row level security;
alter table folio_sync.clocks enable row level security;
alter table folio_sync.records enable row level security;
alter table folio_sync.changes enable row level security;
alter table folio_sync.operations enable row level security;
revoke all on all tables in schema folio_sync from public,anon,authenticated;
insert into folio_sync.entities(entity,key_columns,data_columns) values
('receipt_files',array['id'],array['id','sha256','original_filename','mime_type','relative_path','size_bytes','created_at','updated_at','version','sync_state']),
('expenses',array['id'],array['id','receipt_id','occurred_at','merchant_name','total_amount_minor','tax_amount_minor','currency','category','description','status','extraction_confidence','field_meta','created_at','updated_at','version','sync_state','original_currency','original_total_amount_minor','original_tax_amount_minor','exchange_rate','exchange_rate_date','premises']),
('extraction_runs',array['id'],array['id','expense_id','provider','raw_text','result_json','error','created_at']),
('expense_claims',array['id'],array['id','claim_number','title','description','status','currency','created_at','updated_at','version','sync_state']),
('claim_expenses',array['claim_id','expense_id'],array['claim_id','expense_id','created_at','updated_at','version','sync_state']),
('audit_events',array['id'],array['id','event_type','entity_id','details','created_at']),
('exchange_rates',array['base','quote','requested_date'],array['base','quote','requested_date','rate_date','rate','source','fetched_at']),
('workspaces',array['id'],array['id','kind','name','base_currency','created_at','updated_at','version','sync_state']),
('categories',array['id'],array['id','workspace_id','kind','name','archived_at','created_at','updated_at','version','sync_state']),
('accounts',array['id'],array['id','workspace_id','name','kind','currency','opening_balance_minor','goal_target_minor','goal_target_date','loan_direction','loan_principal_minor','loan_annual_rate','counterparty','note','archived_at','created_at','updated_at','version','sync_state']),
('bills',array['id'],array['id','workspace_id','name','kind','amount_minor','account_id','category_id','payee','note','frequency','interval_count','anchor_on','ends_on','remind_days_before','reminders_enabled','cancelled_on','created_at','updated_at','version','sync_state']),
('transactions',array['id'],array['id','workspace_id','kind','account_id','amount_minor','to_account_id','to_amount_minor','base_amount_minor','exchange_rate','exchange_rate_date','occurred_on','category_id','payee','note','payment_method','source','expense_id','bill_id','bill_due_on','created_at','updated_at','version','sync_state']),
('budgets',array['id'],array['id','workspace_id','category_id','amount_minor','alert_percent','archived_at','created_at','updated_at','version','sync_state']),
('budget_alerts',array['budget_id','period','threshold'],array['budget_id','period','threshold','spent_minor','created_at','delivered_at']),
('transaction_suggestions',array['id'],array['id','workspace_id','source','fingerprint','kind','amount_minor','currency','occurred_on','payee','app_label','status','transaction_id','created_at','updated_at']),
('business_profiles',array['workspace_id'],array['workspace_id','data','next_invoice']),
('customers',array['id'],array['id','workspace_id','data']),
('inventory_items',array['id'],array['id','workspace_id','sku','data']),
('invoices',array['id'],array['id','workspace_id','number','customer_id','status','data']),
('stock_movements',array['id'],array['id','workspace_id','item_id','request_id','quantity_milli','occurred_on','reason','invoice_id','created_at']),
('invoice_payments',array['id'],array['id','workspace_id','invoice_id','request_id','account_id','transaction_id','amount_minor','paid_on','note']),
('receipt_item_reviews',array['expense_id'],array['expense_id','data']);

-- All changes, including trusted dashboard edits, enter the same durable feed.
-- A per-owner row lock serializes revisions until commit, preventing cursor gaps.
create function folio_sync.capture() returns trigger language plpgsql security definer set search_path='' as $$
declare row_data jsonb; old_data jsonb; owner uuid; record_key jsonb; keys text[]; next_seq bigint; next_rev bigint;
begin
 if TG_OP='DELETE' then row_data:=to_jsonb(old); else row_data:=to_jsonb(new); end if;
 owner:=(row_data->>'owner_id')::uuid;
 -- Auth account deletion cascades must not recreate clocks referencing a deleted user.
 if not exists(select 1 from auth.users where id=owner) then
  if TG_OP='DELETE' then return old; end if; return new;
 end if;
 select key_columns into keys from folio_sync.entities where entity=TG_TABLE_NAME;
 if TG_OP='UPDATE' then
  old_data:=to_jsonb(old);
  if new.owner_id<>old.owner_id then raise exception 'Owner cannot change' using errcode='42501'; end if;
  if exists(select 1 from unnest(keys) k where old_data->k is distinct from row_data->k) then raise exception 'Record identity cannot change'; end if;
  if old_data=row_data then return new; end if;
 end if;
 select jsonb_object_agg(k,row_data->k) into record_key from unnest(keys) k;
 insert into folio_sync.clocks(owner_id) values(owner) on conflict do nothing;
 update folio_sync.clocks set seq=seq+1 where owner_id=owner returning seq into next_seq;
 select revision+1 into next_rev from folio_sync.records where owner_id=owner and entity=TG_TABLE_NAME and key=record_key;
 next_rev:=coalesce(next_rev,1);
 insert into folio_sync.records(owner_id,entity,key,revision,seq,data,deleted)
 values(owner,TG_TABLE_NAME,record_key,next_rev,next_seq,case when TG_OP='DELETE' then null else row_data-'owner_id' end,TG_OP='DELETE')
 on conflict(owner_id,entity,key) do update set revision=excluded.revision,seq=excluded.seq,data=excluded.data,deleted=excluded.deleted;
 insert into folio_sync.changes(owner_id,seq,entity,key,revision,data,deleted)
 select owner_id,seq,entity,key,revision,data,deleted from folio_sync.records where owner_id=owner and entity=TG_TABLE_NAME and key=record_key;
 if TG_OP='DELETE' then return old; end if;
 return new;
end $$;

-- Seed a durable baseline before capture triggers begin. No imported values change.
do $$
declare e record; r record; k jsonb; next_seq bigint;
begin
 for e in select * from folio_sync.entities loop
  for r in execute format('select to_jsonb(t) as data from public.%I t',e.entity) loop
   select jsonb_object_agg(col,r.data->col) into k from unnest(e.key_columns) col;
   insert into folio_sync.clocks(owner_id) values((r.data->>'owner_id')::uuid) on conflict do nothing;
   update folio_sync.clocks set seq=seq+1 where owner_id=(r.data->>'owner_id')::uuid returning seq into next_seq;
   insert into folio_sync.records values((r.data->>'owner_id')::uuid,e.entity,k,1,next_seq,r.data-'owner_id',false);
   insert into folio_sync.changes(owner_id,seq,entity,key,revision,data,deleted) select owner_id,seq,entity,key,revision,data,deleted from folio_sync.records where owner_id=(r.data->>'owner_id')::uuid and entity=e.entity and key=k;
  end loop;
  execute format('create trigger folio_cloud_capture after insert or update or delete on public.%I for each row execute function folio_sync.capture()',e.entity);
 end loop;
end $$;
-- Defer relational checks until the entire aggregate batch has been written.
do $$ declare f record; begin
 for f in select conrelid::regclass as tbl,conname from pg_constraint where contype='f' and connamespace='public'::regnamespace loop
  execute format('alter table %s alter constraint %I deferrable initially immediate',f.tbl,f.conname);
 end loop;
end $$;

create function folio_sync.push(changes jsonb) returns jsonb language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); ch jsonb; e folio_sync.entities; saved folio_sync.operations; remote folio_sync.records;
 cid uuid; fingerprint text; results jsonb:='[]'; conflict_found boolean:=false; cols text; updates text; keys text; input_keys text[]; base bigint; result jsonb; affected bigint; duplicate_ids uuid[]:=array[]::uuid[];
begin
 if actor is null then raise exception 'Authentication required' using errcode='42501'; end if;
 if jsonb_typeof(changes) is distinct from 'array' or jsonb_array_length(changes)>2000 or octet_length(changes::text)>4194304 then raise exception 'Invalid batch'; end if;
 insert into folio_sync.clocks(owner_id) values(actor) on conflict do nothing;
 perform 1 from folio_sync.clocks where owner_id=actor for update;
 -- Preflight every operation before writes: conflicts never partially apply an aggregate.
 if (select count(*)<>count(distinct value->>'changeId') from jsonb_array_elements(changes)) then raise exception 'Duplicate operation ID'; end if;
 if exists(select 1 from jsonb_array_elements(changes) c group by c->>'entity',c->'key' having count(*)>1) then raise exception 'Duplicate record in batch'; end if;
 for ch in select value from jsonb_array_elements(changes) loop
  cid:=(ch->>'changeId')::uuid;
  if cid is null then raise exception 'Missing change ID'; end if;
  fingerprint:=encode(sha256(convert_to(ch::text,'UTF8')),'hex');
  select * into saved from folio_sync.operations where owner_id=actor and change_id=cid;
  if found then
   if saved.fingerprint<>fingerprint then raise exception 'Operation ID reused'; end if;
   continue;
  end if;
  select * into e from folio_sync.entities where entity=ch->>'entity';
  if not found or jsonb_typeof(ch->'key') is distinct from 'object' or ch->>'operation' is null or ch->>'operation' not in ('upsert','delete') or ch->>'baseRevision' is null then raise exception 'Invalid operation'; end if;
  base:=(ch->>'baseRevision')::bigint;
  if base<0 then raise exception 'Invalid base revision'; end if;
  select array_agg(k order by k) into input_keys from jsonb_object_keys(ch->'key') k;
  if input_keys is distinct from (select array_agg(k order by k) from unnest(e.key_columns) k) or exists(select 1 from jsonb_each(ch->'key') kv where kv.value='null'::jsonb) then raise exception 'Invalid record key'; end if;
  if ch->>'operation'='upsert' then
   if jsonb_typeof(ch->'data') is distinct from 'object' or not ((ch->'data') @> (ch->'key')) then raise exception 'Invalid data or identity'; end if;
   select array_agg(k order by k) into input_keys from jsonb_object_keys(ch->'data') k;
   if input_keys is distinct from (select array_agg(k order by k) from unnest(e.data_columns) k) then raise exception 'Unknown or missing column'; end if;
  end if;
  if e.entity='receipt_files' and ch->>'operation'='upsert' and (
   exists(select 1 from public.receipt_files f where f.owner_id=actor and f.sha256=ch->'data'->>'sha256' and f.id<>ch->'key'->>'id')
   or exists(select 1 from jsonb_array_elements(changes) other where other->>'entity'='receipt_files' and other->>'operation'='upsert' and other->'data'->>'sha256'=ch->'data'->>'sha256' and other->'key'->>'id'<>ch->'key'->>'id')
  ) then duplicate_ids:=array_append(duplicate_ids,cid); conflict_found:=true; end if;
  select * into remote from folio_sync.records where owner_id=actor and entity=e.entity and key=ch->'key';
  if coalesce(remote.revision,0)<>base or coalesce(remote.deleted,false) then conflict_found:=true; end if;
 end loop;
 if conflict_found then
  for ch in select value from jsonb_array_elements(changes) loop
   select * into saved from folio_sync.operations where owner_id=actor and change_id=(ch->>'changeId')::uuid;
   if found then results:=results||jsonb_build_array(saved.result); continue; end if;
   select * into remote from folio_sync.records where owner_id=actor and entity=ch->>'entity' and key=ch->'key';
   results:=results||jsonb_build_array(jsonb_build_object('changeId',ch->>'changeId','status',case when (ch->>'changeId')::uuid=any(duplicate_ids) then 'rejected' when coalesce(remote.revision,0)<>(ch->>'baseRevision')::bigint or coalesce(remote.deleted,false) then 'conflict' else 'blocked' end,'reason',case when (ch->>'changeId')::uuid=any(duplicate_ids) then 'duplicate_receipt' else null end,'existingReceiptId',case when (ch->>'changeId')::uuid=any(duplicate_ids) then (select min(id) from public.receipt_files f where f.owner_id=actor and f.sha256=ch->'data'->>'sha256' and f.id<>ch->'key'->>'id') else null end,'revision',coalesce(remote.revision,0),'record',case when remote.owner_id is null then null else to_jsonb(remote)-'owner_id' end));
  end loop;
  return results;
 end if;
 set constraints all deferred;
 for ch in select value from jsonb_array_elements(changes) loop
  cid:=(ch->>'changeId')::uuid;fingerprint:=encode(sha256(convert_to(ch::text,'UTF8')),'hex');
  select * into saved from folio_sync.operations where owner_id=actor and change_id=cid;
  if found then results:=results||jsonb_build_array(saved.result);continue;end if;
  select * into e from folio_sync.entities where entity=ch->>'entity';
  if ch->>'operation'='delete' then
   execute format('delete from public.%I t where owner_id=$1 and to_jsonb(t) @> $2',e.entity) using actor,ch->'key';
   get diagnostics affected=row_count;
   if affected=0 then
    -- A delete of an absent local record still establishes a tombstone.
    update folio_sync.clocks set seq=seq+1 where owner_id=actor;
    insert into folio_sync.records(owner_id,entity,key,revision,seq,data,deleted)
    select actor,e.entity,ch->'key',1,seq,null,true from folio_sync.clocks where owner_id=actor;
    insert into folio_sync.changes(owner_id,seq,entity,key,revision,data,deleted) select owner_id,seq,entity,key,revision,data,deleted from folio_sync.records where owner_id=actor and entity=e.entity and key=ch->'key';
   end if;
  else
   select string_agg(format('%I',k),',') into keys from unnest(e.key_columns) k;
   select string_agg(format('%I=excluded.%I',k,k),',') into updates from unnest(e.data_columns) k where not(k=any(e.key_columns));
   -- Identity-only rows still use a no-op identity assignment to satisfy SQL syntax.
   updates:=coalesce(updates,format('%I=excluded.%I',e.key_columns[1],e.key_columns[1]));
   execute format('insert into public.%I select * from jsonb_populate_record(null::public.%I,$1) on conflict(owner_id,%s) do update set %s',e.entity,e.entity,keys,updates) using (ch->'data')||jsonb_build_object('owner_id',actor);
  end if;
  select * into remote from folio_sync.records where owner_id=actor and entity=e.entity and key=ch->'key';
  result:=jsonb_build_object('changeId',cid,'status','applied','revision',remote.revision,'record',to_jsonb(remote)-'owner_id');
  insert into folio_sync.operations values(actor,cid,fingerprint,result);
  results:=results||jsonb_build_array(result);
 end loop;
 set constraints all immediate;
 -- Prevent cross-workspace account/category references (owner FKs alone are insufficient).
 if exists(select 1 from public.transactions t join public.accounts a on a.owner_id=t.owner_id and a.id=t.account_id where t.owner_id=actor and a.workspace_id<>t.workspace_id)
 or exists(select 1 from public.transactions t join public.accounts a on a.owner_id=t.owner_id and a.id=t.to_account_id where t.owner_id=actor and (a.workspace_id<>t.workspace_id or (a.currency=(select currency from public.accounts where owner_id=actor and id=t.account_id) and t.amount_minor<>t.to_amount_minor)))
 or exists(select 1 from public.transactions t join public.categories c on c.owner_id=t.owner_id and c.id=t.category_id where t.owner_id=actor and (c.workspace_id<>t.workspace_id or c.kind<>t.kind))
 or exists(select 1 from public.bills b join public.accounts a on a.owner_id=b.owner_id and a.id=b.account_id where b.owner_id=actor and a.workspace_id<>b.workspace_id)
 or exists(select 1 from public.categories c join public.budgets b on b.owner_id=c.owner_id and b.category_id=c.id where c.owner_id=actor and (c.workspace_id<>b.workspace_id or c.kind<>'expense'))

 or exists(select 1 from public.bills b join public.categories c on c.owner_id=b.owner_id and c.id=b.category_id where b.owner_id=actor and (c.workspace_id<>b.workspace_id or c.kind<>'expense'))
 or exists(select 1 from public.transactions t join public.bills b on b.owner_id=t.owner_id and b.id=t.bill_id where t.owner_id=actor and (b.workspace_id<>t.workspace_id or t.kind<>'expense' or t.source<>'bill'))
 or exists(select 1 from public.invoices i join public.customers c on c.owner_id=i.owner_id and c.id=i.customer_id where i.owner_id=actor and c.workspace_id<>i.workspace_id)
 or exists(select 1 from public.stock_movements m join public.inventory_items i on i.owner_id=m.owner_id and i.id=m.item_id where m.owner_id=actor and i.workspace_id<>m.workspace_id)
 or exists(select 1 from public.stock_movements m join public.invoices i on i.owner_id=m.owner_id and i.id=m.invoice_id where m.owner_id=actor and i.workspace_id<>m.workspace_id)
 or exists(select 1 from public.invoice_payments p join public.invoices i on i.owner_id=p.owner_id and i.id=p.invoice_id where p.owner_id=actor and i.workspace_id<>p.workspace_id)
 or exists(select 1 from public.invoice_payments p join public.accounts a on a.owner_id=p.owner_id and a.id=p.account_id where p.owner_id=actor and a.workspace_id<>p.workspace_id)
 or exists(select 1 from public.invoice_payments p join public.transactions t on t.owner_id=p.owner_id and t.id=p.transaction_id where p.owner_id=actor and (t.workspace_id<>p.workspace_id or t.kind<>'income' or t.source<>'invoice' or t.account_id<>p.account_id or t.amount_minor<>p.amount_minor))
 or exists(select 1 from public.claim_expenses ce join public.expenses ex on ex.owner_id=ce.owner_id and ex.id=ce.expense_id join public.expense_claims c on c.owner_id=ce.owner_id and c.id=ce.claim_id where ce.owner_id=actor and ex.currency is not null and ex.currency<>c.currency)
 or exists(select 1 from public.transactions where owner_id=actor and bill_id is not null group by bill_id,bill_due_on having count(*)>1)
 or exists(select 1 from public.budgets where owner_id=actor and archived_at is null group by workspace_id,coalesce(category_id,'') having count(*)>1)
 or exists(select 1 from public.categories where owner_id=actor group by workspace_id,kind,lower(name) having count(*)>1)
 then raise exception 'Invalid financial graph'; end if;
 return results;
end $$;

create function folio_sync.pull(since bigint,page_limit integer) returns jsonb language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); records jsonb; cursor_value bigint; cutoff bigint; cutoff_batch text;
begin
 if actor is null then raise exception 'Authentication required' using errcode='42501'; end if;
 if since is null or since<0 or page_limit is null or page_limit not between 1 and 500 then raise exception 'Invalid cursor'; end if;
 select seq,batch into cutoff,cutoff_batch from(select seq,batch from folio_sync.changes where owner_id=actor and seq>since order by seq limit page_limit)c order by seq desc limit 1;
 select max(seq) into cutoff from folio_sync.changes where owner_id=actor and batch=cutoff_batch;
 select coalesce(jsonb_agg(to_jsonb(c)-'owner_id'-'batch' order by seq),'[]'),coalesce(max(seq),since) into records,cursor_value
 from(select * from folio_sync.changes where owner_id=actor and seq>since and seq<=cutoff order by seq)c;
 return jsonb_build_object('records',records,'cursor',cursor_value,'hasMore',exists(select 1 from folio_sync.changes where owner_id=actor and seq>cursor_value));
end $$;
create function public.folio_sync_push(changes jsonb) returns jsonb language sql security invoker set search_path='' as $$select folio_sync.push(changes)$$;
create function public.folio_sync_pull(since bigint,page_limit integer default 200) returns jsonb language sql security invoker set search_path='' as $$select folio_sync.pull(since,page_limit)$$;
-- Internal security-definer functions have an explicit authenticated-actor check, fixed
-- search path and narrow grants. They never accept an owner supplied by the client.
grant usage on schema folio_sync to authenticated;
revoke all on all functions in schema folio_sync from public,anon,authenticated;
grant execute on function folio_sync.push(jsonb),folio_sync.pull(bigint,integer) to authenticated;
revoke all on function public.folio_sync_push(jsonb),public.folio_sync_pull(bigint,integer) from public,anon,authenticated;
grant execute on function public.folio_sync_push(jsonb),public.folio_sync_pull(bigint,integer) to authenticated;

-- Keep asset objects immutable. Files can upload before metadata; only a validated
-- manifest makes them discoverable to another device. Garbage collection is deferred.
insert into storage.buckets(id,name,public,file_size_limit) values('folio-receipts','folio-receipts',false,26214400) on conflict(id) do update set public=false,file_size_limit=26214400;
create policy folio_sync_asset_insert on storage.objects for insert to authenticated with check(bucket_id='folio-receipts' and (storage.foldername(name))[1]=(select auth.uid())::text and array_length(storage.foldername(name),1)=2 and name !~ '\.\.');
create policy folio_sync_asset_read on storage.objects for select to authenticated using(bucket_id='folio-receipts' and (storage.foldername(name))[1]=(select auth.uid())::text);
alter table public.receipt_assets add column registered_at timestamptz not null default clock_timestamp();
alter table public.receipt_assets drop constraint receipt_assets_bucket_id_check;
alter table public.receipt_assets add check(bucket_id in ('folio-import','folio-receipts'));
-- Deleting receipt metadata invalidates the discoverable manifest, retaining immutable bytes.
alter table public.receipt_assets drop constraint receipt_assets_owner_id_receipt_id_fkey;
alter table public.receipt_assets add foreign key(owner_id,receipt_id) references public.receipt_files(owner_id,id) on delete cascade deferrable initially immediate;
create function folio_sync.asset_register(receipt_id text,relative_path text,object_path text,sha256 text,size_bytes bigint) returns jsonb language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); r public.receipt_assets; expected text; object_size bigint;
begin
 if actor is null then raise exception 'Authentication required' using errcode='42501'; end if;
 if sha256 is null or sha256 !~ '^[a-f0-9]{64}$' or size_bytes is null or size_bytes not between 1 and 26214400
 or receipt_id is null or receipt_id !~ '^[a-zA-Z0-9-]+$' or relative_path is null or relative_path !~ ('^receipts/'||receipt_id||'/[a-zA-Z0-9_.-]+$') or relative_path ~ '\.\.'
 or object_path is null or object_path !~ ('^'||actor::text||'/'||receipt_id||'/'||sha256||'\.[a-zA-Z0-9]+$') then raise exception 'Invalid receipt asset'; end if;
 if not exists(select 1 from public.receipt_files f where f.owner_id=actor and f.id=receipt_id) then raise exception 'Receipt unavailable'; end if;
 if exists(select 1 from public.receipt_files f where f.owner_id=actor and f.id=receipt_id and f.relative_path=asset_register.relative_path and (f.sha256<>asset_register.sha256 or f.size_bytes<>asset_register.size_bytes)) then raise exception 'Original metadata mismatch'; end if;
 select (metadata->>'size')::bigint into object_size from storage.objects o where o.bucket_id='folio-receipts' and o.name=object_path;
 if object_size is distinct from size_bytes then raise exception 'Object not uploaded or size mismatch'; end if;
 insert into public.receipt_assets(owner_id,receipt_id,bucket_id,object_path,source_relative_path,sha256,size_bytes)
 values(actor,receipt_id,'folio-receipts',object_path,relative_path,sha256,size_bytes)
 on conflict on constraint receipt_assets_pkey do update set registered_at=clock_timestamp();
 select * into r from public.receipt_assets a where a.owner_id=actor and a.object_path=asset_register.object_path;
 if r.receipt_id<>receipt_id or r.source_relative_path<>relative_path or r.sha256<>sha256 or r.size_bytes<>size_bytes then raise exception 'Immutable asset identity mismatch'; end if;
 return to_jsonb(r)-'owner_id';
end $$;
create function public.folio_asset_register(receipt_id text,relative_path text,object_path text,sha256 text,size_bytes bigint) returns jsonb language sql security invoker set search_path='' as $$select folio_sync.asset_register(receipt_id,relative_path,object_path,sha256,size_bytes)$$;
create function public.folio_assets() returns jsonb language sql stable security invoker set search_path='' as $$select coalesce(jsonb_agg(jsonb_build_object('receipt_id',receipt_id,'relative_path',source_relative_path,'object_path',object_path,'sha256',sha256,'size_bytes',size_bytes,'bucket_id',bucket_id)),'[]') from (select distinct on(receipt_id,source_relative_path) * from public.receipt_assets where owner_id=(select auth.uid()) order by receipt_id,source_relative_path,registered_at desc,object_path desc) a$$;
revoke all on function folio_sync.asset_register(text,text,text,text,bigint),public.folio_asset_register(text,text,text,text,bigint),public.folio_assets() from public,anon,authenticated;
grant execute on function folio_sync.asset_register(text,text,text,text,bigint),public.folio_asset_register(text,text,text,text,bigint),public.folio_assets() to authenticated;
