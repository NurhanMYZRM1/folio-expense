-- Run against local Supabase or postgres-bootstrap fixture with psql ON_ERROR_STOP.
begin;
create function pg_temp.assert_true(ok boolean,label text) returns void language plpgsql as $$begin if ok is distinct from true then raise exception 'FAILED: %',label;end if;end$$;
create function pg_temp.assert_error(statement text, expected_state text, label text) returns void language plpgsql as $$begin
 begin execute statement; exception when others then
  if sqlstate=expected_state then return; end if; raise;
 end;
 raise exception 'FAILED: % did not refuse the operation',label;
end$$;
insert into auth.users(id) values('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'),('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb'),('cccccccc-cccc-cccc-cccc-cccccccccccc');
select set_config('request.jwt.claim.sub','aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',true);
set local role authenticated;
select public.create_cloud_workspace('11111111-1111-1111-1111-111111111111','personal','Personal');
select public.create_cloud_workspace('22222222-2222-2222-2222-222222222222','business','Shop');
select pg_temp.assert_error($q$select public.invite_workspace_member('11111111-1111-1111-1111-111111111111','editor')$q$,'P0001','personal workspaces cannot be shared');
select pg_temp.assert_error($q$select public.invite_workspace_member('22222222-2222-2222-2222-222222222222',null)$q$,'42501','null invitation role');
select pg_temp.assert_true((select count(*)=2 from public.cloud_workspaces),'owner sees own workspaces');
do $$declare denied boolean:=false;begin begin insert into public.workspace_members values('11111111-1111-1111-1111-111111111111','bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb','owner');exception when insufficient_privilege then denied:=true;end;if not denied then raise exception 'Direct membership insert allowed';end if;end$$;
select public.sync_push('11111111-1111-1111-1111-111111111111','[
{"changeId":"00000000-0000-0000-0000-000000000001","workspaceId":"11111111-1111-1111-1111-111111111111","entity":"accounts","id":"a0000000-0000-0000-0000-000000000001","baseVersion":0,"operation":"upsert","data":{"id":"a0000000-0000-0000-0000-000000000001","workspace_id":"11111111-1111-1111-1111-111111111111","kind":"bank","name":"Bank","currency":"MYR","opening_balance_minor":0,"note":"","created_at":"2026-11-05T00:00:00Z","updated_at":"2026-11-05T00:00:00Z","version":1,"sync_state":"local_only"}},
{"changeId":"00000000-0000-0000-0000-000000000002","workspaceId":"11111111-1111-1111-1111-111111111111","entity":"transactions","id":"b0000000-0000-0000-0000-000000000001","baseVersion":0,"operation":"upsert","data":{"id":"b0000000-0000-0000-0000-000000000001","workspace_id":"11111111-1111-1111-1111-111111111111","kind":"income","account_id":"a0000000-0000-0000-0000-000000000001","amount_minor":2500,"occurred_on":"2026-11-05","source":"manual","note":"","created_at":"2026-11-05T00:00:00Z","updated_at":"2026-11-05T00:00:00Z","version":1,"sync_state":"local_only"}}]') as result \gset first_
select pg_temp.assert_true((:'first_result'::jsonb->0->>'status')='applied','valid batch applies');
select pg_temp.assert_true((select count(*)=2 from public.sync_records),'two records persisted');
select pg_temp.assert_true((public.sync_push('11111111-1111-1111-1111-111111111111',jsonb_build_array(jsonb_build_object('changeId','00000000-0000-0000-0000-000000000002','workspaceId','11111111-1111-1111-1111-111111111111','entity','transactions','id','b0000000-0000-0000-0000-000000000001','baseVersion',0,'operation','upsert','data',(select data from public.sync_records where entity='transactions'))))->0->>'status')='applied','same change retries idempotently');
select pg_temp.assert_true((select version=1 from public.sync_records where entity='transactions'),'retry does not increment version');
select pg_temp.assert_true((public.sync_push('11111111-1111-1111-1111-111111111111',jsonb_build_array(jsonb_build_object('changeId','00000000-0000-0000-0000-000000000003','workspaceId','11111111-1111-1111-1111-111111111111','entity','transactions','id','b0000000-0000-0000-0000-000000000001','baseVersion',0,'operation','upsert','data',(select data||'{"amount_minor":3000}'::jsonb from public.sync_records where entity='transactions'))))->0->>'status')='conflict','stale edit conflicts');
select pg_temp.assert_true((public.sync_push('11111111-1111-1111-1111-111111111111','[{"changeId":"00000000-0000-0000-0000-000000000004","workspaceId":"11111111-1111-1111-1111-111111111111","entity":"accounts","id":"a0000000-0000-0000-0000-000000000001","baseVersion":1,"operation":"delete","data":{}}]')->0->>'status')='rejected','deleting referenced account rejects whole graph');
select pg_temp.assert_true((select deleted_at is null from public.sync_records where entity='accounts'),'rejected graph rolls back account delete');
select pg_temp.assert_true((public.sync_push('11111111-1111-1111-1111-111111111111','[{"changeId":"00000000-0000-0000-0000-000000000005","workspaceId":"11111111-1111-1111-1111-111111111111","entity":"transactions","id":"b0000000-0000-0000-0000-000000000001","baseVersion":0,"operation":"delete","data":{}}]')->0->>'status')='applied','delete wins concurrent edit');
select pg_temp.assert_true((public.sync_push('11111111-1111-1111-1111-111111111111','[{"changeId":"00000000-0000-0000-0000-000000000006","workspaceId":"11111111-1111-1111-1111-111111111111","entity":"transactions","id":"b0000000-0000-0000-0000-000000000001","baseVersion":2,"operation":"upsert","data":{}}]')->0->>'status')='rejected','stale client cannot resurrect tombstone');
select pg_temp.assert_true(jsonb_array_length(public.sync_pull('11111111-1111-1111-1111-111111111111',0,100))=2,'pull includes tombstones');
select public.invite_workspace_member('22222222-2222-2222-2222-222222222222','editor')->>'code' as code \gset editor_
select public.invite_workspace_member('22222222-2222-2222-2222-222222222222','viewer')->>'code' as code \gset viewer_
select public.invite_workspace_member('22222222-2222-2222-2222-222222222222','viewer')->>'code' as code \gset expired_
select public.invite_workspace_member('22222222-2222-2222-2222-222222222222','editor')->>'code' as code \gset revoked_
select public.revoke_workspace_invitation('22222222-2222-2222-2222-222222222222',:'revoked_code');
reset role;
update folio_private.invitations set expires_at=now()-interval '1 second' where code_hash=extensions.digest(:'expired_code','sha256');
select set_config('request.jwt.claim.sub','bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',true);
set local role authenticated;
select pg_temp.assert_true((select count(*)=0 from public.sync_records),'outsider RLS cannot read records');
do $$declare denied boolean:=false;begin begin perform public.invite_workspace_member('22222222-2222-2222-2222-222222222222','editor');exception when insufficient_privilege then denied:=true;end;if not denied then raise exception 'Outsider created a workspace invitation';end if;end$$;
select pg_temp.assert_true((public.join_workspace(:'expired_code')->>'error')='invalid_invitation','expired invitation refused');
select pg_temp.assert_true((public.join_workspace(:'revoked_code')->>'error')='invalid_invitation','revoked invitation refused');
select pg_temp.assert_true((public.join_workspace('000000000000')->>'error')='invalid_invitation','wrong code refused');
select pg_temp.assert_true((public.join_workspace(:'editor_code')->>'role')='editor','valid invitation joins editor');
select pg_temp.assert_true((select count(*)=1 from public.cloud_workspaces),'member sees only shared business workspace');
do $$declare denied boolean:=false;begin begin perform public.sync_pull('11111111-1111-1111-1111-111111111111',0,100);exception when insufficient_privilege then denied:=true;end;if not denied then raise exception 'Cross-workspace pull allowed';end if;end$$;
select set_config('request.jwt.claim.sub','cccccccc-cccc-cccc-cccc-cccccccccccc',true);
select pg_temp.assert_true((public.join_workspace(:'editor_code')->>'error')='invalid_invitation','used invitation is single use');
select pg_temp.assert_true((public.join_workspace(:'viewer_code')->>'role')='viewer','viewer joins');
do $$declare denied boolean:=false;begin begin perform public.sync_push('22222222-2222-2222-2222-222222222222','[]');exception when insufficient_privilege then denied:=true;end;if not denied then raise exception 'Viewer write allowed';end if;end$$;
select set_config('request.jwt.claim.sub','aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',true);
select public.remove_workspace_member('22222222-2222-2222-2222-222222222222','bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb');
select set_config('request.jwt.claim.sub','bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',true);
select pg_temp.assert_true((select count(*)=0 from public.cloud_workspaces),'revoked membership immediately removes read access');
select pg_temp.assert_error($q$select public.sync_pull('22222222-2222-2222-2222-222222222222',0,100)$q$,'42501','removed member cannot pull');
select pg_temp.assert_error($q$select public.invite_workspace_member('22222222-2222-2222-2222-222222222222','editor')$q$,'42501','removed member cannot create invitation');
do $$declare denied boolean:=false;begin begin perform public.sync_push('22222222-2222-2222-2222-222222222222','[]');exception when insufficient_privilege then denied:=true;end;if not denied then raise exception 'Revoked member write allowed';end if;end$$;
reset role;
insert into auth.users(id) values('dddddddd-dddd-dddd-dddd-dddddddddddd');
select set_config('request.jwt.claim.sub','dddddddd-dddd-dddd-dddd-dddddddddddd',true);
set local role authenticated;
do $$begin for i in 1..10 loop
 perform pg_temp.assert_true((public.join_workspace('000000000000')->>'error')='invalid_invitation','wrong code attempt');
end loop;end$$;
select pg_temp.assert_true((public.join_workspace('000000000000')->>'error')='rate_limited','eleventh join attempt rate limited');
reset role;
update folio_private.join_attempts set window_start=now()-interval '2 hours' where user_id='dddddddd-dddd-dddd-dddd-dddddddddddd';
set local role authenticated;
select pg_temp.assert_true((public.join_workspace('000000000000')->>'error')='invalid_invitation','rate limit resets after one hour');
reset role;
select pg_temp.assert_true(not has_table_privilege('anon','public.sync_records','select'),'anonymous role lacks table grant');
select pg_temp.assert_true(not has_function_privilege('anon','public.sync_push(uuid,jsonb)','execute'),'anonymous role lacks push execute');
select pg_temp.assert_true(not has_function_privilege('authenticated','folio_private.validate_graph(uuid)','execute'),'validator not directly callable');
rollback;
