import { useEffect, useRef, useState, type FormEvent } from 'react';
import { Cloud, RefreshCw, LogOut } from 'lucide-react';
import { Panel } from '../../components/ui';
import { useWorkspace } from '../../app/providers';
import { api, desktop, type CloudConflict } from '../../lib/ipc';
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
export function CloudSync() {
  const {
    cloud,
    cloudBusy,
    cloudError,
    syncCloud,
    signInCloud,
    signOutCloud,
    resolveCloudConflict,
  } = useWorkspace();
  const [email, setEmail] = useState('paragonxplays@gmail.com');
  const [codeSent, setCodeSent] = useState(false);
  const [requesting, setRequesting] = useState(false);
  const [message, setMessage] = useState('');
  const [localError, setLocalError] = useState('');
  const [conflicts, setConflicts] = useState<CloudConflict[]>([]);
  const [reviewing, setReviewing] = useState<string | null>(null);
  const code = useRef<HTMLInputElement>(null);
  const busy = cloudBusy || requesting;

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
  async function resolve(id: string, choice: 'local' | 'remote') {
    setLocalError('');
    try {
      await resolveCloudConflict(id, choice);
      setConflicts(await api.cloudConflicts());
      setReviewing(null);
      await syncCloud();
    } catch (error) {
      setLocalError(errorMessage(error));
    }
  }
  if (!desktop) return null;
  const error = localError || cloudError || cloud?.error;
  return (
    <Panel title="Cloud sync" action={<Cloud size={17} />}>
      <div className="settings-fields">
        <p className="field-help">
          Sign in to automatically upload records and receipt files to your private Supabase
          workspace and receive changes from your other devices. Local work remains available
          offline. Sync runs while Folio is open and resumes when you return.
        </p>
        {cloud?.signedIn ? (
          <>
            <p>
              <strong>Connected as {cloud.email}</strong>
            </p>
            <div role="status" aria-live="polite">
              {cloudBusy
                ? 'Synchronizing records and files…'
                : cloud.pending
                  ? `${cloud.pending} changes waiting to sync.`
                  : cloud.conflicts
                    ? 'Some changes need your review.'
                    : 'No local changes waiting to sync.'}
            </div>
            <p className="field-help">
              Last successful sync:{' '}
              {cloud.lastSync ? new Date(cloud.lastSync).toLocaleString() : 'Not yet completed'}
              {cloud.conflicts > 0 && ` · ${cloud.conflicts} conflicts need review`}
            </p>
            <div className="credential-input">
              <button
                type="button"
                className="button secondary"
                disabled={busy}
                onClick={() => {
                  setLocalError('');
                  void syncCloud();
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
              Signing out pauses cloud transfers. Pending edits stay on this device.
            </p>
          </>
        ) : (
          <form onSubmit={codeSent ? verifyCode : requestCode}>
            <label className="field">
              <span>Email</span>
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
            <p className="field-help">
              This installation is bound to the owner of the imported workspace. Signing in enables
              automatic uploads of existing and new records and receipt files.
            </p>
          </form>
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
      </div>
    </Panel>
  );
}
