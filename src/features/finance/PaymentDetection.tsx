import { useCallback, useState } from 'react';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { useWorkspace } from '../../app/providers';
import { Panel } from '../../components/ui';
import { QueryError, useQuery } from './shared';

/** Android notification access is an explicit device-only opt-in. */
export function PaymentDetection({ onSaved }: { onSaved: (enabled: boolean) => void }) {
  const { settings, setSettings } = useWorkspace();
  const native = (
    window as unknown as {
      __folio?: { invoke: (command: string, args: string) => Promise<string> };
    }
  ).__folio;
  const [version, setVersion] = useState(0),
    [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  const query = useQuery(
    useCallback(async () => {
      void version;
      if (!native) return null;
      const response = JSON.parse(await native.invoke('read_platform_info', '{}')) as {
        result?: { platform: string; paymentAccess: boolean };
        error?: { message: string };
      };
      if (response.error) throw new Error(response.error.message);
      return response.result ?? null;
    }, [native, version]),
  );
  async function toggle(enabled: boolean) {
    setBusy(true);
    setError('');
    try {
      const saved = await api.saveSettings({ ...settings, paymentAlertsEnabled: enabled });
      setSettings(saved);
      onSaved(saved.paymentAlertsEnabled);
      if (enabled && !query.data?.paymentAccess) {
        const response = JSON.parse(await native!.invoke('request_payment_access', '{}')) as {
          error?: { message: string };
        };
        if (response.error) throw new Error(response.error.message);
      }
      setVersion((v) => v + 1);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  if (query.data?.platform !== 'android') return null;
  return (
    <Panel title="Android payment suggestions">
      <div className="settings-fields">
        <p className="field-help">
          Folio can read supported Maybank and Touch ’n Go completed debit notifications locally and
          suggest transactions for your review. Unrelated notifications are ignored. Raw
          notification text is never saved or uploaded. You confirm every transaction before it
          reaches your ledger.
        </p>
        <p className="field-help">
          Folio setting: {settings.paymentAlertsEnabled ? 'enabled' : 'disabled'} · Android
          notification access: {query.data.paymentAccess ? 'allowed' : 'not allowed'}
        </p>
        <QueryError error={error || query.error} />
        <div className="finance-row-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() => void toggle(!settings.paymentAlertsEnabled)}
          >
            {busy
              ? 'Updating…'
              : settings.paymentAlertsEnabled
                ? 'Disable payment suggestions'
                : 'Enable payment suggestions'}
          </button>
          <button
            type="button"
            className="text-button"
            disabled={busy}
            onClick={() => setVersion((v) => v + 1)}
          >
            Check notification access
          </button>
          {settings.paymentAlertsEnabled && !query.data.paymentAccess && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => void toggle(true)}
            >
              Open Android access settings
            </button>
          )}
        </div>
        <p className="field-help">
          These controls save immediately. Android access must also be granted in device Settings.
          Turn it off here at any time.
        </p>
      </div>
    </Panel>
  );
}
