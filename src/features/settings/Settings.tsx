import { useEffect, useRef, useState, type FormEvent } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { ShieldCheck, Monitor, KeyRound, FolderOpen, Save, LockKeyhole, Info } from 'lucide-react';
import { PageHeader, Panel } from '../../components/ui';
import { useWorkspace } from '../../app/providers';
import { api, desktop } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { CURRENCIES } from '../../lib/money';
import type { AppInfo, Settings as Preferences } from '../../bindings/generated';
function Toggle({
  checked,
  onChange,
  label,
  detail,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  detail: string;
  disabled?: boolean;
}) {
  return (
    <label className="toggle-row">
      <span>
        <strong>{label}</strong>
        <small>{detail}</small>
      </span>
      <input
        className="toggle-input"
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        disabled={disabled}
      />
      <span className="toggle-track" />
    </label>
  );
}
export function Settings() {
  const { settings, setSettings, notify } = useWorkspace();
  const [form, setForm] = useState(settings),
    [info, setInfo] = useState<AppInfo | null>(null),
    [busy, setBusy] = useState(false);
  const credential = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (desktop)
      void api
        .info()
        .then(setInfo)
        .catch((e) => notify(errorMessage(e), true));
  }, [notify]);
  function update<K extends keyof Preferences>(key: K, value: Preferences[K]) {
    setForm((f) => ({ ...f, [key]: value }));
  }
  async function save(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      const value = await api.saveSettings(form);
      setSettings(value);
      setForm(value);
      setInfo(await api.info());
      notify('Preferences saved locally.');
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  }
  async function saveKey() {
    const input = credential.current;
    if (!input?.value) return;
    setBusy(true);
    const key = input.value;
    input.value = '';
    try {
      await api.setCredential(key);
      setInfo(await api.info());
      notify('API credential stored in your operating system credential vault.');
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  }
  async function chooseDirectory() {
    if (!desktop) return;
    const directory = await open({ directory: true, multiple: false });
    if (typeof directory === 'string') update('exportDirectory', directory);
  }
  return (
    <>
      <PageHeader
        eyebrow="WORKSPACE PREFERENCES"
        title="Settings"
        subtitle="Make this workspace yours. You’re in control of where your data goes."
      />
      <form onSubmit={save} className="settings-layout">
        <div>
          <Panel title="General" action={<Monitor size={17} />}>
            <div className="settings-fields">
              <div className="form-grid">
                <label className="field">
                  <span>Appearance</span>
                  <select value={form.theme} onChange={(e) => update('theme', e.target.value)}>
                    <option value="system">Follow system</option>
                    <option value="light">Light</option>
                    <option value="dark">Dark</option>
                  </select>
                </label>
                <label className="field">
                  <span>Default currency</span>
                  <select
                    value={form.defaultCurrency}
                    onChange={(e) => update('defaultCurrency', e.target.value)}
                  >
                    {CURRENCIES.map((c) => (
                      <option key={c}>{c}</option>
                    ))}
                  </select>
                </label>
              </div>
              <div className="form-grid">
                <label className="field">
                  <span>Company name</span>
                  <input
                    value={form.company}
                    maxLength={200}
                    placeholder="Company / department"
                    onChange={(e) => update('company', e.target.value)}
                  />
                </label>
                <label className="field">
                  <span>Employee name</span>
                  <input
                    value={form.employee}
                    maxLength={200}
                    placeholder="Your name"
                    onChange={(e) => update('employee', e.target.value)}
                  />
                </label>
              </div>
              <p className="field-help">
                Company and employee details appear on generated claim reports.
              </p>
            </div>
          </Panel>
          <Panel title="Receipt extraction" action={<ShieldCheck size={17} />}>
            <div className="settings-fields">
              <Toggle
                label="Offline OCR"
                detail="Read receipts on this device with bundled Tesseract OCR. English model included."
                checked={form.offlineOcrEnabled}
                onChange={(v) => update('offlineOcrEnabled', v)}
              />
              <Toggle
                label="Allow online AI receipt extraction"
                detail="Send receipt images to your configured provider. Disabled by default."
                checked={form.onlineEnabled}
                onChange={(v) => update('onlineEnabled', v)}
              />
              {form.onlineEnabled && (
                <div className="notice-box">
                  <Info size={16} />
                  <span>
                    Enabling online AI allows receipt images to leave this device. Your provider’s
                    processing and retention terms apply. Save these preferences before importing.
                  </span>
                </div>
              )}
              <label className="field">
                <span>AI provider</span>
                <select value={form.provider} onChange={(e) => update('provider', e.target.value)}>
                  <option value="openai_compatible">OpenAI-compatible vision API</option>
                </select>
              </label>
              <div className="form-grid">
                <label className="field">
                  <span>API base URL</span>
                  <input
                    type="url"
                    value={form.apiBaseUrl}
                    onChange={(e) => update('apiBaseUrl', e.target.value)}
                  />
                </label>
                <label className="field">
                  <span>Vision model</span>
                  <input
                    value={form.aiModel}
                    maxLength={100}
                    onChange={(e) => update('aiModel', e.target.value)}
                  />
                </label>
              </div>
              <p className="field-help">
                Uses the Chat Completions vision and structured JSON schema interface. Changing the
                provider URL clears the saved credential to prevent accidental disclosure.
              </p>
              <div className="credential-heading">
                <KeyRound size={14} />
                <strong>API credential</strong>
                <span
                  className={`credential-state ${info?.credentialConfigured ? 'configured' : ''}`}
                >
                  {info?.credentialConfigured ? 'Stored securely' : 'Not configured'}
                </span>
              </div>
              <div className="credential-input">
                <input
                  type="password"
                  ref={credential}
                  aria-label="New API credential"
                  autoComplete="off"
                  placeholder={
                    info?.credentialConfigured
                      ? 'Enter a new credential to replace it'
                      : 'Paste your API credential'
                  }
                />
                <button
                  type="button"
                  className="button secondary"
                  disabled={busy || form.apiBaseUrl !== settings.apiBaseUrl}
                  onClick={() => void saveKey()}
                >
                  Store securely
                </button>
                {info?.credentialConfigured && (
                  <button
                    type="button"
                    className="text-button danger-text"
                    disabled={busy}
                    onClick={() =>
                      void api
                        .deleteCredential()
                        .then(() => api.info())
                        .then(setInfo)
                        .catch((e) => notify(errorMessage(e), true))
                    }
                  >
                    Remove
                  </button>
                )}
              </div>
              <p className="field-help">
                Write-only input. Saved in macOS Keychain or Windows Credential Manager; never
                returned to the interface or stored in SQLite. Save the provider URL before adding a
                credential.
              </p>
            </div>
          </Panel>
          <Panel title="Claim exports" action={<FolderOpen size={17} />}>
            <div className="settings-fields">
              <Toggle
                label="Include receipt images in PDF"
                detail="Add a receipt appendix so reports remain useful on another computer."
                checked={form.includeReceipts}
                onChange={(v) => update('includeReceipts', v)}
              />
              <label className="field">
                <span>Additional export directory</span>
                <div className="directory-input">
                  <input readOnly value={form.exportDirectory ?? 'App-owned exports folder only'} />
                  <button
                    type="button"
                    className="button secondary"
                    onClick={() =>
                      void chooseDirectory().catch((e) => notify(errorMessage(e), true))
                    }
                  >
                    Choose folder
                  </button>
                  {form.exportDirectory && (
                    <button
                      type="button"
                      className="text-button"
                      onClick={() => update('exportDirectory', null)}
                    >
                      Reset
                    </button>
                  )}
                </div>
              </label>
              <p className="field-help">
                A local copy is always saved inside Folio. An additional copy can be written to your
                chosen folder.
              </p>
            </div>
          </Panel>
          <div className="settings-save">
            <span>Preferences are saved on this device.</span>
            <button className="button primary" disabled={busy} type="submit">
              <Save size={15} />
              {busy ? 'Saving…' : 'Save preferences'}
            </button>
          </div>
        </div>
        <aside>
          <div className="privacy-summary">
            <span className="privacy-summary-icon">
              <LockKeyhole size={24} />
            </span>
            <h2>Local comes first.</h2>
            <p>
              Your computer is the source of truth. Receipts, expenses, claims, and audit history
              live here.
            </p>
            <ul>
              <li>
                <ShieldCheck size={14} />
                No account required
              </li>
              <li>
                <ShieldCheck size={14} />
                No cloud sync required
              </li>
              <li>
                <ShieldCheck size={14} />
                Online extraction is optional
              </li>
            </ul>
          </div>
          <section className="panel storage-info">
            <h3>Application storage</h3>
            <p>Database and original receipts</p>
            <code>{info?.dataDirectory ?? 'Available in the desktop application'}</code>
            <span>Folio version {info?.version ?? '0.1.0'}</span>
            <p className="field-help">
              For a backup, close Folio and copy the entire data directory to a secure location.
            </p>
          </section>
        </aside>
      </form>
    </>
  );
}
