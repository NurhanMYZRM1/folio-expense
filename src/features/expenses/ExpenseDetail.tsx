import { useEffect, useState, type FormEvent } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ArrowLeft, Check, RotateCcw, Save, ShieldCheck } from 'lucide-react';
import type { Expense, ExpenseEdit } from '../../bindings/generated';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { useWorkspace } from '../../app/providers';
import { CATEGORIES } from '../../lib/constants';
import { CURRENCIES, moneyInput, parseMoney } from '../../lib/money';
import { Confidence, Loading, StatusBadge } from '../../components/ui';
import { ReceiptViewer } from '../../components/ReceiptViewer';
function Editor({ initial, reload }: { initial: Expense; reload: () => Promise<void> }) {
  const { notify, refresh, expenses } = useWorkspace();
  const [form, setForm] = useState({
    merchant: initial.merchantName ?? '',
    date: initial.occurredAt ?? '',
    total: moneyInput(initial.totalAmountMinor, initial.currency ?? 'MYR'),
    tax: moneyInput(initial.taxAmountMinor, initial.currency ?? 'MYR'),
    currency: initial.currency ?? 'MYR',
    category: initial.category,
    description: initial.description,
  });
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [dirty, setDirty] = useState(false),
    [changed, setChanged] = useState<Set<string>>(new Set());
  const latest = expenses.find((e) => e.id === initial.id),
    stale = latest && latest.version !== initial.version;
  const locked = ['submitted', 'archived'].includes(initial.status);
  function update(key: keyof typeof form, value: string) {
    setForm((f) => ({ ...f, [key]: value }));
    setChanged((old) => new Set([...old, key]));
    setDirty(true);
  }
  useEffect(() => {
    const listener = (e: BeforeUnloadEvent) => {
      if (dirty) {
        e.preventDefault();
      }
    };
    window.addEventListener('beforeunload', listener);
    return () => window.removeEventListener('beforeunload', listener);
  }, [dirty]);
  async function save(ready: boolean) {
    setBusy(true);
    setError('');
    try {
      const edit: ExpenseEdit = {
        id: initial.id,
        version: initial.version,
        merchantName: form.merchant.trim() || null,
        occurredAt: form.date || null,
        totalAmountMinor: parseMoney(form.total, form.currency),
        taxAmountMinor: parseMoney(form.tax, form.currency),
        currency: form.currency,
        category: form.category,
        description: form.description,
        markReady: ready,
      };
      await api.editExpense(edit);
      setDirty(false);
      await refresh();
      await reload();
      notify(ready ? 'Expense reviewed and ready to claim.' : 'Expense saved on this device.');
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function extract() {
    try {
      await api.queueExtraction(initial.id);
      notify('Extraction queued. Your manual corrections will be kept.');
      await refresh();
    } catch (e) {
      notify(errorMessage(e), true);
    }
  }
  function submit(e: FormEvent) {
    e.preventDefault();
    void save(true);
  }
  const meta = (key: string, formKey: string = key) =>
    changed.has(formKey) ? { source: 'manual' as const, confidence: 1 } : initial.fieldMeta[key];
  return (
    <>
      <div className="detail-title">
        <div>
          <Link className="back-link" to="/expenses">
            <ArrowLeft size={14} />
            All expenses
          </Link>
          <h1>{initial.merchantName || 'Review expense'}</h1>
          <span className="muted">{initial.receiptFilename || 'Manually recorded expense'}</span>
        </div>
        <StatusBadge status={initial.status} />
      </div>
      <div className="expense-detail-grid">
        <ReceiptViewer receiptId={initial.receiptId} />
        <section className="panel expense-form-panel">
          <div className="panel-heading">
            <h2>Expense details</h2>
            <span className="panel-meta">{dirty ? 'Unsaved changes' : 'Saved locally'}</span>
          </div>
          {stale && (
            <div className="notice-box">
              Background processing updated this expense.{' '}
              <button className="text-button" onClick={() => void reload()}>
                Reload latest values{dirty ? ' (discards unsaved edits)' : ''}
              </button>
            </div>
          )}
          {error && <div className="inline-error">{error}</div>}
          {locked && (
            <div className="notice-box">
              This expense belongs to a {initial.status} claim.{' '}
              <Link to={`/claims/${initial.claimId}`}>Open claim to reopen it.</Link>
            </div>
          )}
          <form onSubmit={submit}>
            <fieldset disabled={locked || busy}>
              <label className="field">
                <span>
                  Merchant <Confidence meta={meta('merchantName', 'merchant')} />
                </span>
                <input
                  value={form.merchant}
                  maxLength={300}
                  onChange={(e) => update('merchant', e.target.value)}
                  placeholder="Merchant or supplier name"
                />
              </label>
              <div className="form-grid">
                <label className="field">
                  <span>
                    Expense date <Confidence meta={meta('date')} />
                  </span>
                  <input
                    type="date"
                    value={form.date}
                    onChange={(e) => update('date', e.target.value)}
                  />
                </label>
                <label className="field">
                  <span>
                    Currency <Confidence meta={meta('currency')} />
                  </span>
                  <select
                    value={form.currency}
                    onChange={(e) => update('currency', e.target.value)}
                  >
                    {CURRENCIES.map((c) => (
                      <option key={c}>{c}</option>
                    ))}
                  </select>
                </label>
              </div>
              <div className="form-grid">
                <label className="field">
                  <span>
                    Total amount <Confidence meta={meta('total')} />
                  </span>
                  <div className="money-input">
                    <span>{form.currency}</span>
                    <input
                      inputMode="decimal"
                      placeholder="0.00"
                      value={form.total}
                      onChange={(e) => update('total', e.target.value)}
                    />
                  </div>
                </label>
                <label className="field">
                  <span>
                    Tax included <Confidence meta={meta('tax')} />
                  </span>
                  <div className="money-input">
                    <span>{form.currency}</span>
                    <input
                      inputMode="decimal"
                      placeholder="Optional"
                      value={form.tax}
                      onChange={(e) => update('tax', e.target.value)}
                    />
                  </div>
                </label>
              </div>
              <label className="field">
                <span>
                  Category <Confidence meta={meta('category')} />
                </span>
                <select value={form.category} onChange={(e) => update('category', e.target.value)}>
                  {CATEGORIES.map((c) => (
                    <option key={c}>{c}</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>
                  Business purpose / notes <span className="optional">Optional</span>
                </span>
                <textarea
                  value={form.description}
                  maxLength={4000}
                  rows={4}
                  placeholder="What was this expense for?"
                  onChange={(e) => update('description', e.target.value)}
                />
              </label>
              <div className="form-tip">
                <ShieldCheck size={16} />
                <span>
                  Manual corrections stay yours. Future extraction runs will never overwrite them.
                </span>
              </div>
              <div className="form-actions">
                <button
                  type="button"
                  className="button secondary"
                  disabled={busy || !!stale}
                  onClick={() => void save(false)}
                >
                  <Save size={15} />
                  Save draft
                </button>
                <button type="submit" className="button primary" disabled={busy || !!stale}>
                  <Check size={16} />
                  {busy ? 'Saving…' : 'Mark as ready'}
                </button>
              </div>
            </fieldset>
          </form>
          {initial.receiptId && !locked && (
            <div className="extraction-footer">
              <button
                className="text-button"
                disabled={busy || dirty || initial.status === 'extracting'}
                onClick={() => void extract()}
              >
                <RotateCcw size={13} />
                Run extraction again
              </button>
              <span>Low-confidence fields are highlighted for review.</span>
            </div>
          )}
        </section>
      </div>
    </>
  );
}
export function ExpenseDetail() {
  const { id } = useParams();
  const [expense, setExpense] = useState<Expense | null>(null),
    [error, setError] = useState('');
  const { notify } = useWorkspace();
  async function load() {
    if (!id) return;
    try {
      setExpense(await api.expense(id));
    } catch (e) {
      setError(errorMessage(e));
      notify(errorMessage(e), true);
    }
  }
  useEffect(() => {
    setExpense(null);
    void load();
  }, [id]);
  return expense ? (
    <Editor key={`${expense.id}-${expense.version}`} initial={expense} reload={load} />
  ) : error ? (
    <div className="inline-error">{error}</div>
  ) : (
    <Loading />
  );
}
