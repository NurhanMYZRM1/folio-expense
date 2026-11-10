import { useState } from 'react';
import { Panel } from '../../components/ui';
import { api, desktop } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import type { DataExport as ExportResult } from '../../bindings/generated';

export function DataExport() {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ExportResult | null>(null);
  const [error, setError] = useState('');
  async function create() {
    setBusy(true);
    setError('');
    setResult(null);
    try {
      setResult(await api.exportAllData());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Panel title="Export all my data">
      <div className="settings-fields">
        <p>
          Save a ZIP containing all personal and business records as JSON, a database snapshot,
          original receipts, and existing reports. It works offline.
        </p>
        <p className="field-help">
          The archive is not encrypted. Store it somewhere protected. Credentials, logs, cache files
          and previous data archives are excluded. Restore instructions are inside.
        </p>
        {error && (
          <p className="inline-error" role="alert">
            {error}
          </p>
        )}
        <button
          type="button"
          className="button secondary"
          disabled={busy || !desktop}
          onClick={() => void create()}
        >
          {busy ? 'Creating data archive…' : 'Create data archive'}
        </button>
        <div aria-live="polite">
          {result && (
            <>
              <p>
                Saved {result.files} files to <code>{result.path}</code>
              </p>
              <button
                type="button"
                className="text-button"
                onClick={() => {
                  setError('');
                  void api.openDataExport(result.id).catch((e) => setError(errorMessage(e)));
                }}
              >
                Open archive location / share
              </button>
            </>
          )}
        </div>
        {!desktop && (
          <p className="field-help">Open Folio on your computer or phone to export your data.</p>
        )}
      </div>
    </Panel>
  );
}
