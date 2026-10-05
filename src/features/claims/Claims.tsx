import { useEffect, useState, type FormEvent } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import {
  ArrowLeft,
  Plus,
  Files,
  ArrowUpRight,
  Download,
  Minus,
  Send,
  Archive,
  RotateCcw,
  Save,
  FileText,
  FileSpreadsheet,
} from 'lucide-react';
import { PageHeader, Panel, StatusBadge, EmptyState, Loading } from '../../components/ui';
import { ExpenseTable } from '../../components/ExpenseTable';
import { useWorkspace } from '../../app/providers';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { CURRENCIES, formatMoney } from '../../lib/money';
import { dateLabel } from '../../lib/constants';
import type { ClaimDetail as Detail, ClaimStatus, CsvExport } from '../../bindings/generated';
import { AnimatePresence, motion } from 'motion/react';
import { insertMotion, listItem, spring } from '../../components/motion';
export function Claims() {
  const { claims, settings, notify, refresh } = useWorkspace();
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false),
    [title, setTitle] = useState(''),
    [currency, setCurrency] = useState(settings.defaultCurrency),
    [busy, setBusy] = useState(false),
    [filter, setFilter] = useState('');
  async function create(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      const c = await api.createClaim(title, currency);
      await refresh();
      navigate(`/claims/${c.id}`);
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  }
  const filtered = claims.filter((c) => !filter || c.status === filter);
  return (
    <>
      <PageHeader
        eyebrow="CLAIM PACKAGES"
        title="Claims"
        subtitle="Bring your expenses together. Prepare a report that’s ready to share."
        actions={
          <button className="button primary" onClick={() => setCreating(!creating)}>
            <Plus size={16} />
            New claim
          </button>
        }
      />
      <AnimatePresence initial={false}>
        {creating && (
          <motion.form className="panel new-claim-form" onSubmit={create} {...insertMotion}>
            <label className="field">
              <span>Claim title</span>
              <input
                autoFocus
                required
                maxLength={200}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder="e.g. September client visits"
              />
            </label>
            <label className="field">
              <span>Currency</span>
              <select value={currency} onChange={(e) => setCurrency(e.target.value)}>
                {CURRENCIES.map((c) => (
                  <option key={c}>{c}</option>
                ))}
              </select>
            </label>
            <button className="button secondary" type="button" onClick={() => setCreating(false)}>
              Cancel
            </button>
            <button className="button primary" disabled={busy || !title.trim()} type="submit">
              Create claim
            </button>
          </motion.form>
        )}
      </AnimatePresence>
      <div className="expense-tabs">
        {['', 'draft', 'submitted', 'archived'].map((s) => (
          <button className={filter === s ? 'active' : ''} key={s} onClick={() => setFilter(s)}>
            {s ? s[0].toUpperCase() + s.slice(1) : 'All claims'}{' '}
            <span>{claims.filter((c) => !s || c.status === s).length}</span>
            {filter === s && (
              <motion.i
                className="tab-underline"
                layoutId="claims-tab"
                transition={spring.indicator}
              />
            )}
          </button>
        ))}
      </div>
      {filtered.length ? (
        <div className="claim-grid">
          <AnimatePresence mode="popLayout" initial={false}>
            {filtered.map((c, i) => (
              <motion.div key={c.id} {...listItem(i)} layout>
                <Link className="claim-card" to={`/claims/${c.id}`}>
                  <div className="claim-card-top">
                    <span className="claim-icon">
                      <Files size={21} />
                    </span>
                    <StatusBadge status={c.status} />
                  </div>
                  <span className="claim-number">{c.claimNumber}</span>
                  <h2>{c.title}</h2>
                  <p>
                    {c.expenseCount} expenses · Created {dateLabel(c.createdAt)}
                  </p>
                  <div className="claim-card-bottom">
                    <strong>{formatMoney(c.totalAmountMinor, c.currency)}</strong>
                    <ArrowUpRight size={18} />
                  </div>
                </Link>
              </motion.div>
            ))}
          </AnimatePresence>
        </div>
      ) : (
        <section className="panel">
          <EmptyState
            title={filter ? `No ${filter} claims` : 'Make your first claim'}
            description="Create a claim, add your imported receipts, and export a complete PDF report."
            action={
              <button className="button primary" onClick={() => setCreating(true)}>
                <Plus size={15} />
                Create a claim
              </button>
            }
          />
        </section>
      )}
    </>
  );
}
export function ClaimDetail() {
  const { id } = useParams();
  const { expenses, jobs, settings, notify, refresh } = useWorkspace();
  const [detail, setDetail] = useState<Detail | null>(null),
    [error, setError] = useState(''),
    [title, setTitle] = useState(''),
    [description, setDescription] = useState(''),
    [busy, setBusy] = useState(false),
    [adding, setAdding] = useState(false),
    [picked, setPicked] = useState<Set<string>>(new Set()),
    [csvBusy, setCsvBusy] = useState(false),
    [xlsxBusy, setXlsxBusy] = useState(false),
    [csvExport, setCsvExport] = useState<CsvExport | null>(null),
    [xlsxExport, setXlsxExport] = useState<CsvExport | null>(null);
  async function load() {
    if (!id) return;
    try {
      const d = await api.claim(id);
      setDetail(d);
      setTitle(d.claim.title);
      setDescription(d.claim.description);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  useEffect(() => {
    void load();
  }, [id]);
  async function act(action: () => Promise<unknown>, message?: string) {
    setBusy(true);
    try {
      await action();
      await load();
      await refresh();
      if (message) notify(message);
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  }
  if (!detail) return error ? <div className="inline-error">{error}</div> : <Loading />;
  const { claim } = detail,
    draft = claim.status === 'draft';
  // Every imported expense not yet in a claim. Ready and needs-review ones
  // in the claim's currency can be added now (submitting still waits until
  // all are reviewed); the rest are listed with what they are waiting for.
  const unclaimed = expenses.filter(
    (e) => !e.claimId && !['submitted', 'archived'].includes(e.status),
  );
  const available = unclaimed.filter(
    (e) => ['ready', 'needs_review'].includes(e.status) && e.currency === claim.currency,
  );
  const unavailable = unclaimed.filter((e) => !available.includes(e));
  const pickedAvailable = available.filter((e) => picked.has(e.id));
  function waitingFor(e: (typeof expenses)[number]) {
    if (['draft', 'extracting'].includes(e.status)) return 'Still being read';
    if (!e.currency) return 'No currency yet, so open it to review';
    if (
      settings.currencyConversionEnabled &&
      claim.currency === settings.defaultCurrency &&
      e.currency !== claim.currency
    )
      return `In ${e.currency}, waiting for an exchange rate (connect to the internet)`;
    return `In ${e.currency}, but this claim is in ${claim.currency}`;
  }
  async function addExpenses(ids: string[]) {
    await act(
      () => api.addClaimExpenses(claim.id, ids),
      `Added ${ids.length} expense${ids.length === 1 ? '' : 's'} to the claim.`,
    );
    setPicked(new Set());
  }
  const exports = jobs.filter((j) => j.jobType === 'generate_pdf' && j.entityId === claim.id);
  const exporting = exports.some((j) => ['pending', 'running'].includes(j.status));
  async function transition(status: ClaimStatus) {
    await act(
      () => api.transitionClaim(claim.id, status),
      status === 'submitted'
        ? 'Claim marked as submitted locally. No data was sent.'
        : 'Claim status updated.',
    );
  }
  async function exportCsv() {
    setCsvBusy(true);
    try {
      const csv = await api.exportClaimCsv(claim.id);
      setCsvExport(csv);
      notify(`Saved ${csv.fileName}.`);
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setCsvBusy(false);
    }
  }
  async function exportXlsx() {
    setXlsxBusy(true);
    try {
      const workbook = await api.exportClaimXlsx(claim.id);
      setXlsxExport(workbook);
      notify(`Saved ${workbook.fileName}.`);
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setXlsxBusy(false);
    }
  }
  return (
    <>
      <Link className="back-link" to="/claims">
        <ArrowLeft size={14} />
        All claims
      </Link>
      <PageHeader
        eyebrow={claim.claimNumber}
        title={claim.title}
        subtitle={`Created ${dateLabel(claim.createdAt)} · ${claim.currency} claim package`}
        actions={
          <>
            <StatusBadge status={claim.status} />
            <button
              className="button primary"
              disabled={busy || exporting || !claim.expenseCount}
              onClick={() =>
                void act(
                  () => api.requestPdf(claim.id),
                  'PDF generation queued. Your report will appear below.',
                )
              }
            >
              <Download size={16} />
              {exporting ? 'Generating PDF…' : 'Export PDF'}
            </button>
            <button
              className="button secondary"
              disabled={busy || csvBusy || xlsxBusy || !claim.expenseCount}
              onClick={() => void exportCsv()}
            >
              <FileSpreadsheet size={16} />
              {csvBusy ? 'Exporting CSV…' : 'Export CSV'}
            </button>
            <button
              className="button secondary"
              disabled={busy || csvBusy || xlsxBusy || !claim.expenseCount}
              onClick={() => void exportXlsx()}
            >
              <FileSpreadsheet size={16} />
              {xlsxBusy ? 'Exporting Excel…' : 'Export Excel'}
            </button>
            {csvExport && (
              <span className="inline-export-note">
                Saved {csvExport.fileName} ·{' '}
                <button
                  className="text-button"
                  onClick={() =>
                    void api.openCsvExport(csvExport.id).catch((e) => notify(errorMessage(e), true))
                  }
                >
                  Open CSV
                </button>
              </span>
            )}
            {xlsxExport && (
              <span className="inline-export-note">
                Saved {xlsxExport.fileName} ·{' '}
                <button
                  className="text-button"
                  onClick={() =>
                    void api
                      .openXlsxExport(xlsxExport.id)
                      .catch((e) => notify(errorMessage(e), true))
                  }
                >
                  Open Excel
                </button>
              </span>
            )}
          </>
        }
      />
      <div className="claim-summary-grid">
        <div className="panel claim-summary">
          <span>Claim total</span>
          <strong>{formatMoney(claim.totalAmountMinor, claim.currency)}</strong>
          <small>
            {claim.expenseCount} expenses ·{' '}
            {settings.includeReceipts ? 'Receipt appendix enabled' : 'Summary report only'}
          </small>
        </div>
        <form
          className="panel claim-edit-form"
          onSubmit={(e) => {
            e.preventDefault();
            void act(
              () => api.editClaim(claim.id, claim.version, title, description),
              'Claim details saved.',
            );
          }}
        >
          <label className="field">
            <span>Claim title</span>
            <input
              value={title}
              disabled={!draft || busy}
              maxLength={200}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Description / purpose</span>
            <input
              value={description}
              disabled={!draft || busy}
              maxLength={4000}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional business context"
            />
          </label>
          {draft && (
            <button
              className="button secondary small"
              disabled={busy || (title === claim.title && description === claim.description)}
            >
              <Save size={13} />
              Save details
            </button>
          )}
        </form>
      </div>
      <Panel
        title="Included expenses"
        action={
          draft && (
            <button
              className="button secondary small"
              disabled={busy}
              onClick={() => setAdding(!adding)}
            >
              <Plus size={14} />
              Add expenses
            </button>
          )
        }
      >
        <ExpenseTable
          expenses={detail.expenses}
          actions={
            draft
              ? (e) => (
                  <button
                    className="icon-button danger-text"
                    disabled={busy}
                    title="Remove from claim"
                    aria-label={`Remove ${e.merchantName} from claim`}
                    onClick={() => void act(() => api.setClaimExpense(claim.id, e.id, false))}
                  >
                    <Minus size={16} />
                  </button>
                )
              : undefined
          }
        />
      </Panel>
      <AnimatePresence initial={false}>
        {adding && draft && (
          <motion.div key="adding" {...insertMotion}>
            <Panel
              title="Add from your imported receipts"
              action={
                <button className="text-button" onClick={() => setAdding(false)}>
                  Done
                </button>
              }
            >
              {available.length ? (
                <>
                  <div className="selection-bar">
                    <span>
                      {pickedAvailable.length
                        ? `${pickedAvailable.length} selected`
                        : `${available.length} can be added`}
                    </span>
                    <button
                      className="button primary small"
                      disabled={busy || !pickedAvailable.length}
                      onClick={() => void addExpenses(pickedAvailable.map((e) => e.id))}
                    >
                      <Plus size={13} />
                      Add selected
                    </button>
                    <button
                      className="button secondary small"
                      disabled={busy}
                      onClick={() => void addExpenses(available.map((e) => e.id))}
                    >
                      Add all {available.length}
                    </button>
                    {pickedAvailable.some((e) => e.status === 'needs_review') && (
                      <span className="muted">
                        Expenses that need review can be added now; review them before submitting.
                      </span>
                    )}
                  </div>
                  <ExpenseTable
                    expenses={available}
                    compact
                    selection={{
                      selected: picked,
                      onToggle: (id) =>
                        setPicked((old) => {
                          const next = new Set(old);
                          if (next.has(id)) next.delete(id);
                          else next.add(id);
                          return next;
                        }),
                      onToggleAll: (checked) =>
                        setPicked(checked ? new Set(available.map((e) => e.id)) : new Set()),
                    }}
                    actions={(e) => (
                      <button
                        className="button secondary small"
                        disabled={busy}
                        onClick={() => void addExpenses([e.id])}
                      >
                        <Plus size={13} />
                        Add
                      </button>
                    )}
                  />
                </>
              ) : (
                <div className="quiet-empty">
                  No unclaimed receipts in {claim.currency} yet.{' '}
                  <Link to="/import">Import receipts</Link> to add them here.
                </div>
              )}
              {unavailable.length > 0 && (
                <div className="unavailable-list">
                  <h3>Not available yet</h3>
                  {unavailable.map((e) => (
                    <div className="job-row" key={e.id}>
                      <div>
                        <Link to={`/expenses/${e.id}`}>
                          {e.merchantName || e.receiptFilename || 'Untitled expense'}
                        </Link>
                        <small>{waitingFor(e)}</small>
                      </div>
                      <StatusBadge status={e.status} />
                    </div>
                  ))}
                </div>
              )}
            </Panel>
          </motion.div>
        )}
      </AnimatePresence>
      <Panel
        title="Generated reports"
        action={
          <Link className="text-link" to="/settings">
            Export settings <ArrowUpRight size={13} />
          </Link>
        }
      >
        {exports.length ? (
          <AnimatePresence initial={false}>
            {exports.map((j, i) => (
              <motion.div className="job-row" key={j.id} {...listItem(i)}>
                <FileText size={20} />
                <div>
                  <strong>Claim report · {dateLabel(j.createdAt)}</strong>
                  <small>
                    {j.status === 'completed'
                      ? 'Saved locally · snapshot of claim at export time'
                      : j.lastError || 'Preparing your report and receipt appendix'}
                  </small>
                </div>
                <StatusBadge status={j.status} />
                {j.status === 'completed' && (
                  <button
                    className="button secondary small"
                    onClick={() =>
                      void api.openExport(j.id).catch((e) => notify(errorMessage(e), true))
                    }
                  >
                    Open PDF <ArrowUpRight size={13} />
                  </button>
                )}
                {j.status === 'failed' && (
                  <button
                    className="text-button"
                    onClick={() => void act(() => api.retryJob(j.id))}
                  >
                    Retry
                  </button>
                )}
              </motion.div>
            ))}
          </AnimatePresence>
        ) : (
          <div className="quiet-empty">
            Export a PDF to create a report with receipt references
            {settings.includeReceipts ? ' and a receipt appendix' : ''}.
          </div>
        )}
      </Panel>
      <div className="claim-status-actions">
        <p>Claim status is tracked locally. Marking a claim submitted does not send it anywhere.</p>
        {draft ? (
          <button
            className="button secondary"
            disabled={busy || !claim.expenseCount}
            onClick={() => void transition('submitted')}
          >
            <Send size={14} />
            Mark submitted
          </button>
        ) : (
          <button
            className="button secondary"
            disabled={busy}
            onClick={() => void transition('draft')}
          >
            <RotateCcw size={14} />
            Reopen as draft
          </button>
        )}
        {claim.status !== 'archived' && (
          <button
            className="button secondary"
            disabled={busy}
            onClick={() => void transition('archived')}
          >
            <Archive size={14} />
            Archive claim
          </button>
        )}
      </div>
    </>
  );
}
