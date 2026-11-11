import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Cloud, RefreshCw, LogOut } from 'lucide-react';
import { Modal } from '../../components/motion';
import { Panel } from '../../components/ui';
import { useWorkspace } from '../../app/providers';
import {
  api,
  desktop,
  type CloudConflict,
  type CloudDiagnosis,
  type CloudProvider,
  type CloudStatus,
} from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';

function conflictFields(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return { value };
  const object = value as Record<string, unknown>;
  const inner = object.payload ?? object.row ?? object.data;
  return inner && typeof inner === 'object' && !Array.isArray(inner)
    ? (inner as Record<string, unknown>)
    : object;
}
function ConflictComparison({ local, remote }: { local: unknown; remote: unknown }) {
  const left = conflictFields(local),
    right = conflictFields(remote);
  const fields = [...new Set([...Object.keys(left), ...Object.keys(right)])];
  const display = (value: unknown) =>
    value == null ? '—' : typeof value === 'object' ? JSON.stringify(value) : String(value);
  return (
    <div style={{ overflowX: 'auto', maxHeight: 360 }}>
      <table>
        <caption>Review the saved values before replacing either version</caption>
        <thead>
          <tr>
            <th>Field</th>
            <th>This device</th>
            <th>Cloud</th>
          </tr>
        </thead>
        <tbody>
          {fields.map((field) => (
            <tr key={field}>
              <th scope="row">{field.replaceAll('_', ' ').replace(/minor/g, '(minor units)')}</th>
              <td style={{ overflowWrap: 'anywhere', maxWidth: 250 }}>{display(left[field])}</td>
              <td style={{ overflowWrap: 'anywhere', maxWidth: 250 }}>{display(right[field])}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
const PROVIDERS: Record<string, string> = { email: 'Email', google: 'Google', apple: 'Apple' };
const STATES: Record<CloudStatus['state'], string> = {
  local: 'Not connected. Everything stays on this device.',
  signedOut: 'Signed out. Changes are saved on this device.',
  signInAgain: 'Sign in again to resume sync. Changes are saved on this device.',
  offline: 'Offline — changes are saved on this device and will sync when the connection returns.',
  needsAttention: 'Sync needs your attention.',
  pending: 'Changes are waiting to sync.',
  upToDate: 'No local changes waiting to sync.',
};
const when = (value: string | null) => (value ? new Date(value).toLocaleString() : '—');

/** Each layer of the connection checked on its own; nothing here is a secret. */
function ConnectionDetails() {
  const [details, setDetails] = useState<CloudDiagnosis | null>(null);
  const [checking, setChecking] = useState(false);
  const [failed, setFailed] = useState('');
  async function check() {
    setChecking(true);
    setFailed('');
    try {
      setDetails(await api.cloudDiagnose());
    } catch (error) {
      setFailed(errorMessage(error));
    } finally {
      setChecking(false);
    }
  }
  const rows: [string, string][] = details
    ? [
        ['Environment', details.environment],
        ['Project', details.project],
        ['App key', details.keyKind],
        ['Reaches the cloud', details.reachable],
        ['Signed-in sync', details.authenticated],
        ['Records waiting', String(details.status.pending)],
        ['Receipt files waiting', String(details.pendingFiles)],
        ['Conflicts to review', String(details.status.conflicts)],
        ['Last successful sync', when(details.status.lastSync)],
        ['Next automatic retry', when(details.status.retryAt)],
        ['Last problem', details.status.error ?? '—'],
        ['Request ID', details.status.requestId ?? '—'],
      ]
    : [];
  return (
    <details
      className="cloud-details"
      onToggle={(e) => {
        if (e.currentTarget.open && !details) void check();
      }}
    >
      <summary>Connection details</summary>
      {failed && (
        <div className="inline-error" role="alert">
          {failed}
        </div>
      )}
      {details && (
        <dl>
          {rows.map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}
      <button
        type="button"
        className="text-button"
        disabled={checking}
        onClick={() => void check()}
      >
        <RefreshCw size={14} className={checking ? 'spin' : ''} />
        {checking ? 'Checking…' : 'Check again'}
      </button>
    </details>
  );
}

export function CloudSync() {
  const {
    cloud,
    cloudBusy,
    cloudError,
    cloudBrowser,
    cloudChoice,
    syncCloud,
    signInCloud,
    signInWithProvider,
    cancelProviderSignIn,
    adoptCloudData,
    signOutCloud,
    resolveCloudConflict,
  } = useWorkspace();
  const [typed, setEmail] = useState<string | null>(null);
  const [codeSent, setCodeSent] = useState(false);
  const [requesting, setRequesting] = useState(false);
  const [message, setMessage] = useState('');
  const [localError, setLocalError] = useState('');
  const [conflicts, setConflicts] = useState<CloudConflict[]>([]);
  const [reviewing, setReviewing] = useState<string | null>(null);
  const [available, setAvailable] = useState<CloudProvider[] | null>(null);
  const code = useRef<HTMLInputElement>(null);
  const busy = cloudBusy || requesting;
  // The account these records belong to is offered first; any address can be typed.
  const email = typed ?? cloud?.email ?? '';

  useEffect(() => {
    let active = true;
    if (!cloud?.signedIn || !cloud.conflicts) return;
    void api
      .cloudConflicts()
      .then((rows) => {
        if (active) setConflicts(rows);
      })
      .catch((error) => {
        if (active) setLocalError(errorMessage(error));
      });
    return () => {
      active = false;
    };
  }, [cloud?.signedIn, cloud?.conflicts, cloud?.lastSync]);

  useEffect(() => {
    let active = true;
    // Only methods the project has switched on are offered; if it cannot be
    // reached, both stay and an attempt explains what is wrong.
    void api
      .cloudProviders()
      .then((list) => {
        if (active) setAvailable(list);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  async function requestCode(event: FormEvent) {
    event.preventDefault();
    setRequesting(true);
    setLocalError('');
    setMessage('');
    try {
      await api.cloudRequestCode(email.trim());
      setCodeSent(true);
      setMessage(
        'Check your email. Copy the sign-in link into the field below without opening it. If you already opened it, paste the address your browser then showed.',
      );
    } catch (error) {
      setLocalError(errorMessage(error));
    } finally {
      setRequesting(false);
    }
  }
  async function verifyCode(event: FormEvent) {
    event.preventDefault();
    const value = code.current?.value.trim() ?? '';
    if (!value) return;
    // Never retain codes/passwords in React state or include them in notices.
    if (code.current) code.current.value = '';
    setLocalError('');
    setMessage('');
    try {
      await signInCloud(email.trim(), value, 'code');
      setCodeSent(false);
    } catch (error) {
      setLocalError(errorMessage(error));
    }
  }
  function withProvider(provider: CloudProvider, link = false) {
    setLocalError('');
    setMessage('');
    void signInWithProvider(provider, link).catch((error) => setLocalError(errorMessage(error)));
  }
  async function resolve(id: string, choice: 'local' | 'remote') {
    setLocalError('');
    try {
      await resolveCloudConflict(id, choice);
      setConflicts(await api.cloudConflicts());
      setReviewing(null);
      await syncCloud(false, true);
    } catch (error) {
      setLocalError(errorMessage(error));
    }
  }
  if (!desktop) return null;
  const error = localError || cloudError || cloud?.error;
  const offered = (['google', 'apple'] as const).filter((p) => !available || available.includes(p));
  const missing = offered.filter((p) => !cloud?.providers.includes(p));
  return (
    <Panel title="Cloud sync" action={<Cloud size={17} />}>
      <div className="settings-fields">
        <p className="field-help">
          Sign in with the same account on each device to keep records and receipt files in sync
          through your private cloud workspace. Work stays available offline; sync runs while Folio
          is open and catches up when you return.
        </p>
        {cloud?.signedIn ? (
          <>
            <p>
              <strong>Connected as {cloud.email ?? 'your account'}</strong>
            </p>
            <div role="status" aria-live="polite">
              {cloudBusy
                ? 'Synchronizing records and files…'
                : cloud.state === 'pending'
                  ? `${cloud.pending} changes waiting to sync.`
                  : cloud.conflicts
                    ? 'Some changes need your review.'
                    : STATES[cloud.state]}
            </div>
            <p className="field-help">
              Last successful sync:{' '}
              {cloud.lastSync ? new Date(cloud.lastSync).toLocaleString() : 'Not yet completed'}
              {cloud.conflicts > 0 && ` · ${cloud.conflicts} conflicts need review`}
              {cloud.state === 'offline' &&
                cloud.retryAt &&
                ` · Next automatic try ${new Date(cloud.retryAt).toLocaleTimeString()}`}
            </p>
            <div className="credential-input">
              <button
                type="button"
                className="button secondary"
                disabled={busy}
                onClick={() => {
                  setLocalError('');
                  void syncCloud(false, true);
                }}
              >
                <RefreshCw size={14} className={cloudBusy ? 'spin' : ''} />
                {cloudBusy ? 'Syncing…' : 'Sync now'}
              </button>
              <button
                type="button"
                className="text-button"
                disabled={busy}
                onClick={() => void signOutCloud().catch((e) => setLocalError(errorMessage(e)))}
              >
                <LogOut size={14} /> Sign out
              </button>
            </div>
            <p className="field-help">
              Sign-in methods: {cloud.providers.map((p) => PROVIDERS[p] ?? p).join(', ') || 'Email'}
              . Signing out pauses cloud transfers. Pending edits stay on this device.
            </p>
            {missing.length > 0 && (
              <div className="credential-input">
                {missing.map((provider) => (
                  <button
                    key={provider}
                    type="button"
                    className="text-button"
                    disabled={busy || cloudBrowser}
                    onClick={() => withProvider(provider, true)}
                  >
                    Add {PROVIDERS[provider]} sign-in
                  </button>
                ))}
              </div>
            )}
          </>
        ) : (
          <>
            {cloud?.linked && (
              <p role="status">
                {STATES[cloud.state]} These records belong to{' '}
                {cloud.email ?? 'the account that was connected here'}. Another account opens its
                own separate records.
              </p>
            )}
            <div className="credential-input">
              {offered.map((provider) => (
                <button
                  key={provider}
                  type="button"
                  className="button secondary"
                  disabled={busy || cloudBrowser}
                  onClick={() => withProvider(provider)}
                >
                  Continue with {PROVIDERS[provider]}
                </button>
              ))}
            </div>
            <form onSubmit={codeSent ? verifyCode : requestCode}>
              <label className="field">
                <span>Or sign in by email</span>
                <input
                  type="email"
                  autoComplete="username"
                  value={email}
                  required
                  disabled={busy || codeSent}
                  onChange={(e) => setEmail(e.target.value)}
                />
              </label>
              {codeSent && (
                <label className="field">
                  <span>Sign-in code or email link</span>
                  <input
                    ref={code}
                    type="text"
                    autoComplete="one-time-code"
                    inputMode="text"
                    required
                    maxLength={4096}
                    disabled={busy}
                  />
                </label>
              )}
              <div className="credential-input">
                <button className="button primary" type="submit" disabled={busy}>
                  {busy
                    ? 'Connecting…'
                    : codeSent
                      ? 'Sign in and enable sync'
                      : 'Email me a sign-in link'}
                </button>
                {codeSent ? (
                  <button
                    className="text-button"
                    type="button"
                    disabled={busy}
                    onClick={() => {
                      setCodeSent(false);
                      setMessage('');
                    }}
                  >
                    Use another email or resend
                  </button>
                ) : (
                  // Supabase sends only a few sign-in emails per hour; reuse one already received.
                  <button
                    className="text-button"
                    type="button"
                    disabled={busy}
                    onClick={() => setCodeSent(true)}
                  >
                    I already have a sign-in link
                  </button>
                )}
              </div>
            </form>
          </>
        )}
        {cloudBrowser && (
          <div className="notice-box" role="status">
            <div>
              <strong>Finish signing in in your browser</strong>
              <p>Folio continues here as soon as the browser returns.</p>
              <button type="button" className="text-button" onClick={cancelProviderSignIn}>
                Cancel sign-in
              </button>
            </div>
          </div>
        )}
        {message && !cloud?.signedIn && <p role="status">{message}</p>}
        {error && (
          <div className="inline-error" role="alert">
            {error}
          </div>
        )}
        {(cloud?.signedIn && cloud.conflicts > 0 ? conflicts : []).map((conflict) => (
          <section key={conflict.id} className="notice-box">
            <div>
              <strong>{conflict.entity.replaceAll('_', ' ')} changed on another device</strong>
              <p>Both versions have been kept. Review them before choosing which version to use.</p>
              <button
                type="button"
                className="text-button"
                disabled={busy}
                onClick={() => setReviewing(reviewing === conflict.id ? null : conflict.id)}
              >
                {reviewing === conflict.id ? 'Close comparison' : 'Compare versions'}
              </button>
              {reviewing === conflict.id && (
                <>
                  <ConflictComparison local={conflict.local} remote={conflict.remote} />
                  <div className="credential-input">
                    <button
                      className="button secondary"
                      type="button"
                      disabled={busy}
                      onClick={() => void resolve(conflict.id, 'local')}
                    >
                      Use this device’s version
                    </button>
                    <button
                      className="button secondary"
                      type="button"
                      disabled={busy}
                      onClick={() => void resolve(conflict.id, 'remote')}
                    >
                      Use cloud version
                    </button>
                  </div>
                </>
              )}
            </div>
          </section>
        ))}
        <ConnectionDetails />
      </div>
      <Modal
        open={!!cloudChoice}
        onClose={() => !busy && void adoptCloudData('cancel').catch(() => undefined)}
        labelledBy="cloud-choice-title"
        describedBy="cloud-choice-body"
        role="alertdialog"
      >
        <h2 id="cloud-choice-title">Add this device’s records to your account?</h2>
        <p id="cloud-choice-body">
          This device already holds expenses that belong to no account.
          {cloudChoice?.email ? ` You are signing in as ${cloudChoice.email}.` : ''} Adding them
          saves a backup first, then uploads them and keeps them in sync. Keeping them separate
          leaves them on this device only, and the account opens with its own records.
        </p>
        <div className="modal-actions">
          <button
            className="text-button"
            disabled={busy}
            onClick={() => void adoptCloudData('cancel').catch(() => undefined)}
          >
            Cancel sign-in
          </button>
          <button
            className="button secondary"
            data-autofocus
            disabled={busy}
            onClick={() =>
              void adoptCloudData('separate').catch((e) => setLocalError(errorMessage(e)))
            }
          >
            Keep them separate
          </button>
          <button
            className="button primary"
            disabled={busy}
            onClick={() => void adoptCloudData('join').catch((e) => setLocalError(errorMessage(e)))}
          >
            Add to this account
          </button>
        </div>
      </Modal>
    </Panel>
  );
}
