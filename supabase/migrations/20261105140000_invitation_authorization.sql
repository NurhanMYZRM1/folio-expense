-- NULL is not a role. In PL/pgSQL, `NULL NOT IN (...)` is NULL and an
-- IF with a NULL condition does not run. Refuse missing membership explicitly
-- before this SECURITY DEFINER function creates an invitation.
create or replace function folio_private.invite(w uuid, invited_role text) returns jsonb language plpgsql security definer set search_path='' as $$
declare actor uuid:=auth.uid(); actor_role text; code text:=''; bytes bytea; i integer;
begin
 perform 1 from public.cloud_workspaces where id=w for update;
 actor_role:=folio_private.member_role(w);
 if actor is null or actor_role is null or actor_role not in ('owner','admin')
    or invited_role is null or invited_role not in ('admin','editor','viewer')
    or (invited_role='admin' and actor_role<>'owner') then
  raise exception 'Forbidden' using errcode='42501';
 end if;
 if (select kind from public.cloud_workspaces where id=w)<>'business' then raise exception 'Personal workspaces cannot be shared'; end if;
 bytes:=extensions.gen_random_bytes(12);
 for i in 0..11 loop code:=code||substr('0123456789ABCDEFGHJKMNPQRSTVWXYZ',1+(get_byte(bytes,i)&31),1); end loop;
 insert into folio_private.invitations(code_hash,workspace_id,role,created_by) values(extensions.digest(code,'sha256'),w,invited_role,actor);
 return jsonb_build_object('code',code,'expiresAt',now()+interval '7 days');
end$$;
