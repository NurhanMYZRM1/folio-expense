begin;
create function pg_temp.assert_true(ok boolean,label text) returns void language plpgsql as $$begin if ok is distinct from true then raise exception 'FAILED: %',label;end if;end$$;
select set_config('request.jwt.claim.sub','aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',true);
set local role authenticated;
select pg_temp.assert_true(jsonb_array_length(public.folio_sync_pull(0,200)->'records')=1,'import baseline exists');
select pg_temp.assert_true((public.folio_sync_pull(0,200)->'records'->0->>'revision')::bigint=1,'baseline revision one');
select public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000001","entity":"workspaces","key":{"id":"baseline"},"baseRevision":1,"operation":"upsert","data":{"id":"baseline","kind":"personal","name":"Updated","base_currency":"MYR","created_at":"2026-11-05","updated_at":"2026-11-05","version":2,"sync_state":"local_only"}}]') as result \gset first_
select pg_temp.assert_true((:'first_result'::jsonb->0->>'status')='applied','update applies');
select pg_temp.assert_true((:'first_result'::jsonb->0->>'revision')::bigint=2,'update revision two');
select public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000001","entity":"workspaces","key":{"id":"baseline"},"baseRevision":1,"operation":"upsert","data":{"id":"baseline","kind":"personal","name":"Updated","base_currency":"MYR","created_at":"2026-11-05","updated_at":"2026-11-05","version":2,"sync_state":"local_only"}}]') as result \gset retry_
select pg_temp.assert_true(:'retry_result'::jsonb=:'first_result'::jsonb,'lost ack replay idempotent');
select pg_temp.assert_true((public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000002","entity":"workspaces","key":{"id":"baseline"},"baseRevision":1,"operation":"upsert","data":{"id":"baseline","kind":"personal","name":"Stale","base_currency":"MYR","created_at":"2026-11-05","updated_at":"2026-11-05","version":3,"sync_state":"local_only"}}]')->0->>'status')='conflict','stale edit preserved as conflict');
select pg_temp.assert_true((select name='Updated' from public.workspaces where id='baseline'),'conflict never overwrites');
-- New FK child can precede its parent within one atomic batch.
select public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000003","entity":"categories","key":{"id":"child"},"baseRevision":0,"operation":"upsert","data":{"id":"child","workspace_id":"parent","kind":"expense","name":"Food","archived_at":null,"created_at":"2026-11-05","updated_at":"2026-11-05","version":1,"sync_state":"local_only"}},{"changeId":"00000000-0000-0000-0000-000000000004","entity":"workspaces","key":{"id":"parent"},"baseRevision":0,"operation":"upsert","data":{"id":"parent","kind":"business","name":"Parent","base_currency":"MYR","created_at":"2026-11-05","updated_at":"2026-11-05","version":1,"sync_state":"local_only"}}]') as result \gset aggregate_
select pg_temp.assert_true((:'aggregate_result'::jsonb->0->>'status')='applied','child before parent aggregate works');
-- Deletes are optimistic as well; stale delete must not discard a newer edit.
select pg_temp.assert_true((public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000005","entity":"categories","key":{"id":"child"},"baseRevision":0,"operation":"delete","data":null}]')->0->>'status')='conflict','stale delete conflicts');
select pg_temp.assert_true((public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000006","entity":"categories","key":{"id":"child"},"baseRevision":1,"operation":"delete","data":null}]')->0->>'status')='applied','matching delete works');
select pg_temp.assert_true((public.folio_sync_pull(0,200)->'records'->4->>'deleted')::boolean,'pull retains tombstone');
select pg_temp.assert_true(jsonb_array_length(public.folio_sync_pull(0,1)->'records')=1 and (public.folio_sync_pull(0,1)->>'hasMore')::boolean,'baseline page is separate from current transaction');
select pg_temp.assert_true(jsonb_array_length(public.folio_sync_pull(1,1)->'records')=4 and not (public.folio_sync_pull(1,1)->>'hasMore')::boolean,'pull never splits aggregate transaction');
-- Owner overrides, unknown entities, and malformed keys must be refused.
do $$begin
 begin perform public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000007","entity":"settings","key":{"key":"preferences"},"baseRevision":0,"operation":"delete"}]'); raise exception 'Unknown entity accepted'; exception when raise_exception then if sqlerrm='Unknown entity accepted' then raise;end if;end;
 begin perform public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000008","entity":"workspaces","key":{"id":"wrong","owner_id":"bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"},"baseRevision":0,"operation":"delete"}]'); raise exception 'Owner override accepted'; exception when raise_exception then if sqlerrm='Owner override accepted' then raise;end if;end;
 begin insert into public.workspaces(owner_id,id,kind,name,base_currency,created_at,updated_at) values(auth.uid(),'direct','personal','Direct','MYR','','');raise exception 'Direct write accepted'; exception when insufficient_privilege then null;end;
end $$;


select public.folio_sync_push('[{"changeId":"00000000-0000-0000-0000-000000000020","entity":"workspaces","key":{"id":"baseline"},"baseRevision":0,"operation":"delete","data":null},{"changeId":"00000000-0000-0000-0000-000000000021","entity":"workspaces","key":{"id":"parent"},"baseRevision":1,"operation":"delete","data":null}]') as result \gset conflict_batch_
select pg_temp.assert_true((:'conflict_batch_result'::jsonb->0->>'status')='conflict' and (:'conflict_batch_result'::jsonb->1->>'status')='blocked','one stale row blocks entire aggregate');
select pg_temp.assert_true((select count(*)=1 from public.workspaces where id='parent'),'blocked delete is not applied');
-- Manifest authorization and immutable-file discovery (bytes tested by the real HTTP client).
reset role;
insert into public.receipt_files(owner_id,id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at,updated_at)
values('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa','receipt-test',repeat('a',64),'test.jpg','image/jpeg','receipts/receipt-test/original.jpg',10,'2026-11-05','2026-11-05');
insert into storage.objects(bucket_id,name,metadata) values('folio-receipts','aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/receipt-test/'||repeat('a',64)||'.jpg','{"size":10}');
set local role authenticated;
select public.folio_sync_push(jsonb_build_array(jsonb_build_object('changeId','00000000-0000-0000-0000-000000000030','entity','receipt_files','key',jsonb_build_object('id','duplicate-receipt'),'baseRevision',0,'operation','upsert','data',(select to_jsonb(r)-'owner_id'||jsonb_build_object('id','duplicate-receipt','relative_path','receipts/duplicate-receipt/original.jpg') from public.receipt_files r where id='receipt-test')))) as result \gset duplicate_
select pg_temp.assert_true((:'duplicate_result'::jsonb->0->>'status')='rejected' and (:'duplicate_result'::jsonb->0->>'reason')='duplicate_receipt','duplicate original returns actionable rejection');
select pg_temp.assert_true((select count(*)=0 from public.receipt_files where id='duplicate-receipt'),'duplicate rejection does not mutate');
select public.folio_asset_register('receipt-test','receipts/receipt-test/original.jpg','aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/receipt-test/'||repeat('a',64)||'.jpg',repeat('a',64),10);
select pg_temp.assert_true(jsonb_array_length(public.folio_assets())=1,'owner discovers own manifest');
select pg_temp.assert_true((select count(*)=1 from storage.objects where bucket_id='folio-receipts'),'owner reads own files');
select public.folio_asset_register('receipt-test','receipts/receipt-test/original.jpg','aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa/receipt-test/'||repeat('a',64)||'.jpg',repeat('a',64),10);
select pg_temp.assert_true(jsonb_array_length(public.folio_assets())=1,'repeated manifest registration idempotent');
select set_config('request.jwt.claim.sub','bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',true);
select pg_temp.assert_true(jsonb_array_length(public.folio_sync_pull(0,200)->'records')=0,'other owner has no feed');
select pg_temp.assert_true((select count(*)=0 from public.workspaces),'other owner no rows');
select pg_temp.assert_true(jsonb_array_length(public.folio_assets())=0,'other owner no manifest');
select pg_temp.assert_true((select count(*)=0 from storage.objects),'other owner no objects');
reset role;
select pg_temp.assert_true(not has_function_privilege('anon','public.folio_sync_push(jsonb)','execute'),'anon no push');
select pg_temp.assert_true(not has_table_privilege('authenticated','folio_sync.operations','select'),'internal operation data unavailable');
select 'CLOUD SECURITY TESTS PASSED' as result;
rollback;
