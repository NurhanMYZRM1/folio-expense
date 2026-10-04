import { useCallback, useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { open } from '@tauri-apps/plugin-dialog';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import {
  Upload,
  FileImage,
  FileText,
  ShieldCheck,
  ArrowUpRight,
  LoaderCircle,
  X,
  CheckCircle2,
} from 'lucide-react';
import { PageHeader, Panel, StatusBadge } from '../../components/ui';
import { api, desktop } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { useWorkspace } from '../../app/providers';
import type { ImportOutcome } from '../../bindings/generated';
export function ImportReceipts() {
  const { settings, jobs, expenses, refresh, notify } = useWorkspace();
  const [dragging, setDragging] = useState(false),
    [importing, setImporting] = useState(false),
    [outcomes, setOutcomes] = useState<ImportOutcome[]>([]);
  const ingest = useCallback(
    async (paths: string[]) => {
      if (!paths.length) return;
      setImporting(true);
      try {
        const result = await api.importReceipts(paths);
        setOutcomes((old) => [...result, ...old]);
        await refresh();
        const count = result.filter((r) => r.expenseId).length;
        if (count)
          notify(
            `${count} ${count === 1 ? 'receipt saved' : 'receipts saved'} locally. Processing is queued.`,
          );
      } catch (e) {
        notify(errorMessage(e), true);
      } finally {
        setImporting(false);
      }
    },
    [refresh, notify],
  );
  useEffect(() => {
    if (!desktop) return;
    let unlisten: (() => void) | undefined,
      cancelled = false;
    void getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        if (event.payload.type === 'over' || event.payload.type === 'enter') setDragging(true);
        else {
          setDragging(false);
          if (event.payload.type === 'drop') void ingest(event.payload.paths);
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [ingest]);
  async function pick() {
    if (!desktop) {
      notify('Receipt import is available in the Folio desktop application.', true);
      return;
    }
    try {
      const paths = await open({
        multiple: true,
        directory: false,
        filters: [{ name: 'Receipts', extensions: ['png', 'jpg', 'jpeg', 'heic', 'heif', 'pdf'] }],
      });
      if (paths) await ingest(Array.isArray(paths) ? paths : [paths]);
    } catch (e) {
      notify(errorMessage(e), true);
    }
  }
  const recentJobs = jobs
    .filter((j) => j.jobType !== 'generate_pdf' && j.jobType !== 'future_sync')
    .slice(0, 20);
  return (
    <>
      <PageHeader
        eyebrow="RECEIPT INBOX"
        title="Import receipts"
        subtitle="Drop them in. We’ll save the originals and take care of the first pass."
      />
      <div className="import-layout">
        <div>
          <button
            type="button"
            className={`drop-zone ${dragging ? 'dragging' : ''}`}
            onClick={() => void pick()}
            disabled={importing}
          >
            <span className="upload-illustration">
              {importing ? (
                <LoaderCircle className="spin" size={30} />
              ) : (
                <Upload size={30} strokeWidth={1.4} />
              )}
            </span>
            <strong>
              {importing
                ? 'Securing your receipts…'
                : dragging
                  ? 'Drop to save your receipts'
                  : 'Drag receipts into your workspace'}
            </strong>
            <span>or click to browse files on your computer</span>
            <span className="button primary">Choose receipts</span>
            <small>PNG, JPG, HEIC, PDF · up to 25 MB each · multiple files supported</small>
          </button>
          <div className={`import-privacy ${settings.onlineEnabled ? 'online' : ''}`}>
            <ShieldCheck size={19} />
            <div>
              <strong>
                {settings.onlineEnabled
                  ? 'Online AI extraction is enabled'
                  : 'Private, offline processing'}
              </strong>
              <p>
                {settings.onlineEnabled
                  ? `Receipt images will be sent to ${new URL(settings.apiBaseUrl).hostname} when a credential is available. Local OCR is the fallback.`
                  : 'Receipts and extracted text stay on this device. No account, cloud upload, or connection required.'}
              </p>
            </div>
            <Link to="/settings">
              Settings <ArrowUpRight size={14} />
            </Link>
          </div>
          {outcomes.length > 0 && (
            <Panel
              title="Import results"
              action={
                <button className="text-button" onClick={() => setOutcomes([])}>
                  Dismiss all
                </button>
              }
            >
              {outcomes.map((r, i) => (
                <div
                  className={`import-result ${r.error ? 'has-error' : ''}`}
                  key={`${r.filename}-${i}`}
                >
                  <FileText size={18} />
                  <div>
                    <strong>{r.filename}</strong>
                    <span>
                      {r.error?.message ?? 'Original saved. Extraction and preview queued.'}
                    </span>
                  </div>
                  {r.error?.existingExpenseId ? (
                    <>
                      <Link
                        className="button secondary small"
                        to={`/expenses/${r.error.existingExpenseId}`}
                      >
                        Open existing expense
                      </Link>
                      <button
                        className="icon-button"
                        aria-label="Cancel duplicate import"
                        onClick={() => setOutcomes((v) => v.filter((_, x) => x !== i))}
                      >
                        <X size={14} />
                      </button>
                    </>
                  ) : r.expenseId ? (
                    <Link className="text-link" to={`/expenses/${r.expenseId}`}>
                      Review <ArrowUpRight size={14} />
                    </Link>
                  ) : null}
                </div>
              ))}
            </Panel>
          )}
          <Panel
            title="Processing queue"
            action={
              <span className="panel-meta">
                {jobs.filter((j) => ['pending', 'running'].includes(j.status)).length} active jobs
              </span>
            }
          >
            {recentJobs.length ? (
              recentJobs.map((j) => (
                <div className="job-row" key={j.id}>
                  <span className="job-icon">
                    {j.status === 'completed' ? <CheckCircle2 size={17} /> : <FileText size={17} />}
                  </span>
                  <div>
                    <Link to={`/expenses/${j.entityId}`}>
                      {expenses.find((e) => e.id === j.entityId)?.receiptFilename ?? 'Receipt'}
                    </Link>
                    <small>
                      {j.jobType === 'extract_receipt'
                        ? 'Extract receipt information'
                        : 'Create receipt preview'}
                      {j.lastError && ` · ${j.lastError}`}
                    </small>
                  </div>
                  <StatusBadge status={j.status} />
                  {j.status === 'failed' && (
                    <button
                      className="text-button"
                      onClick={() =>
                        void api
                          .retryJob(j.id)
                          .then(refresh)
                          .catch((e) => notify(errorMessage(e), true))
                      }
                    >
                      Retry
                    </button>
                  )}
                </div>
              ))
            ) : (
              <div className="quiet-empty">
                New receipts will appear here as they are processed.
              </div>
            )}
          </Panel>
        </div>
        <aside className="import-guide">
          <h3>From receipt to ready</h3>
          {[
            {
              n: '01',
              title: 'Save the original',
              text: 'A private copy is stored in your workspace. Your source file stays untouched.',
              icon: FileImage,
            },
            {
              n: '02',
              title: 'Extract the details',
              text: 'Local OCR reads the receipt and suggests key expense information.',
              icon: FileText,
            },
            {
              n: '03',
              title: 'Review & claim',
              text: 'Check uncertain fields, add a business purpose, and group expenses into a claim.',
              icon: CheckCircle2,
            },
          ].map((step) => (
            <div key={step.n}>
              <span className="step-number">{step.n}</span>
              <step.icon size={20} />
              <strong>{step.title}</strong>
              <p>{step.text}</p>
            </div>
          ))}
          <div className="duplicate-note">
            <strong>Already imported?</strong>
            <p>
              Exact duplicates are detected automatically, so one receipt won’t become two expenses.
            </p>
          </div>
        </aside>
      </div>
    </>
  );
}
