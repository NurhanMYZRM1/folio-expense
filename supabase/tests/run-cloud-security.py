#!/usr/bin/env python3
"""Verify the deployed full-domain protocol against the imported mirror in disposable PG17."""
from pathlib import Path
import subprocess,time,uuid
root=Path(__file__).resolve().parents[2]
name='folio-cloud-test-'+uuid.uuid4().hex[:12]
def sql(content,label):
 r=subprocess.run(['docker','exec','-i',name,'psql','-h','127.0.0.1','-U','postgres','-v','ON_ERROR_STOP=1','-q'],input=content,capture_output=True,text=True)
 if r.returncode:raise RuntimeError(label+'\n'+r.stderr)
 print('PASS '+label,flush=True)
try:
 subprocess.run(['docker','run','--rm','-d','--network','none','--name',name,'-e','POSTGRES_HOST_AUTH_METHOD=trust','postgres:17-alpine'],check=True,capture_output=True)
 deadline=time.monotonic()+60
 while time.monotonic()<deadline:
  if subprocess.run(['docker','exec',name,'pg_isready','-h','127.0.0.1','-U','postgres'],capture_output=True).returncode==0:break
  time.sleep(.2)
 else:raise RuntimeError('postgres startup timed out')
 sql((root/'supabase/tests/postgres-bootstrap.sql').read_text(),'standalone fixture')
 sql("alter table storage.objects add column metadata jsonb default '{}';create or replace function storage.foldername(text) returns text[] language sql immutable as $$select (string_to_array($1,'/'))[1:array_length(string_to_array($1,'/'),1)-1]$$;",'storage metadata fixture')
 sql((root/'supabase/imported-v7-schema.sql').read_text(),'imported-v7-schema')
 sql("insert into auth.users values('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'),('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb');insert into public.workspaces(owner_id,id,kind,name,base_currency,created_at,updated_at) values('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa','baseline','personal','Baseline','MYR','2026-11-05','2026-11-05');",'import baseline')
 sql((root/'supabase/automatic-sync.sql').read_text(),'automatic-sync')
 sql((root/'supabase/tests/cloud-security.sql').read_text(),'cloud-security')
finally:subprocess.run(['docker','rm','-f',name],capture_output=True)
