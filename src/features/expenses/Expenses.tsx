import { useState } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import {
  Search,
  Upload,
  Plus,
  ChevronLeft,
  ChevronRight,
  SlidersHorizontal,
  FileSpreadsheet,
} from 'lucide-react';
import { useWorkspace } from '../../app/providers';
import { PageHeader } from '../../components/ui';
import { ExpenseTable } from '../../components/ExpenseTable';
import { CATEGORIES, STATUS_LABELS } from '../../lib/constants';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import type { CsvExport } from '../../bindings/generated';
export function Expenses() {
  const { expenses, notify, refresh } = useWorkspace();
  const navigate = useNavigate(),
    [params] = useSearchParams();
  const [search, setSearch] = useState(''),
    [category, setCategory] = useState(''),
    [status, setStatus] = useState(params.get('status') ?? ''),
    [from, setFrom] = useState(''),
    [to, setTo] = useState(''),
    [sort, setSort] = useState('date-desc'),
    [page, setPage] = useState(1);
  const [selected, setSelected] = useState<Set<string>>(new Set()),
    [csvBusy, setCsvBusy] = useState(false),
    [lastCsv, setLastCsv] = useState<CsvExport | null>(null);
  const filtered = expenses
    .filter(
      (e) =>
        (!search ||
          `${e.merchantName} ${e.description} ${e.receiptFilename}`
            .toLowerCase()
            .includes(search.toLowerCase())) &&
        (!category || e.category === category) &&
        (!status || e.status === status) &&
        (!from || (e.occurredAt ?? '') >= from) &&
        (!to || (e.occurredAt ?? '9999') <= to),
    )
    .sort((a, b) =>
      sort === 'merchant'
        ? (a.merchantName ?? '').localeCompare(b.merchantName ?? '')
        : sort === 'amount'
          ? (a.currency ?? '').localeCompare(b.currency ?? '') ||
            (b.totalAmountMinor ?? 0) - (a.totalAmountMinor ?? 0)
          : sort === 'date-asc'
            ? (a.occurredAt ?? '').localeCompare(b.occurredAt ?? '')
            : (b.occurredAt ?? '').localeCompare(a.occurredAt ?? ''),
    );
  const pages = Math.max(1, Math.ceil(filtered.length / 25)),
    currentPage = Math.min(page, pages);
  const visible = filtered.slice((currentPage - 1) * 25, currentPage * 25);
  function change(fn: (v: string) => void, value: string) {
    fn(value);
    setPage(1);
    setSelected(new Set());
    setLastCsv(null);
  }
  async function create() {
    try {
      const e = await api.createExpense();
      await refresh();
      navigate(`/expenses/${e.id}`);
    } catch (e) {
      notify(errorMessage(e), true);
    }
  }
  function toggleOne(id: string) {
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }
  function toggleAllVisible(checked: boolean) {
    setSelected((s) => {
      const next = new Set(s);
      for (const e of visible) {
        if (checked) next.add(e.id);
        else next.delete(e.id);
      }
      return next;
    });
  }
  function clearSelection() {
    setSelected(new Set());
    setLastCsv(null);
  }
  async function exportSelected() {
    setCsvBusy(true);
    try {
      const csv = await api.exportExpensesCsv([...selected]);
      setLastCsv(csv);
      notify(`Saved ${csv.fileName} · ${csv.rows} expenses.`);
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setCsvBusy(false);
    }
  }
  return (
    <>
      <PageHeader
        eyebrow="EXPENSE REGISTER"
        title="Expenses"
        subtitle="Every receipt and expense, organized in one local workspace."
        actions={
          <>
            <button className="button secondary" onClick={() => void create()}>
              <Plus size={15} />
              Manual expense
            </button>
            <Link className="button primary" to="/import">
              <Upload size={15} />
              Import receipts
            </Link>
          </>
        }
      />
      <div className="expense-tabs">
        <button className={!status ? 'active' : ''} onClick={() => change(setStatus, '')}>
          All expenses <span>{expenses.length}</span>
        </button>
        {['needs_review', 'ready', 'submitted'].map((s) => (
          <button
            className={status === s ? 'active' : ''}
            key={s}
            onClick={() => change(setStatus, s)}
          >
            {STATUS_LABELS[s]} <span>{expenses.filter((e) => e.status === s).length}</span>
          </button>
        ))}
      </div>
      <section className="panel">
        <div className="filters">
          <label className="search-input">
            <Search size={16} />
            <input
              aria-label="Search expenses"
              placeholder="Search merchant, notes, or receipt…"
              value={search}
              onChange={(e) => change(setSearch, e.target.value)}
            />
          </label>
          <select
            aria-label="Filter category"
            value={category}
            onChange={(e) => change(setCategory, e.target.value)}
          >
            <option value="">All categories</option>
            {CATEGORIES.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
          <select
            aria-label="Filter status"
            value={status}
            onChange={(e) => change(setStatus, e.target.value)}
          >
            <option value="">All statuses</option>
            {['draft', 'extracting', 'needs_review', 'ready', 'submitted', 'archived'].map((s) => (
              <option key={s} value={s}>
                {STATUS_LABELS[s]}
              </option>
            ))}
          </select>
          <select aria-label="Sort expenses" value={sort} onChange={(e) => setSort(e.target.value)}>
            <option value="date-desc">Newest first</option>
            <option value="date-asc">Oldest first</option>
            <option value="merchant">Merchant A–Z</option>
            <option value="amount">Currency, then amount</option>
          </select>
        </div>
        <div className="date-filters">
          <SlidersHorizontal size={13} />
          <span>Date range</span>
          <input
            type="date"
            aria-label="From date"
            value={from}
            onChange={(e) => change(setFrom, e.target.value)}
          />
          <span>to</span>
          <input
            type="date"
            aria-label="To date"
            value={to}
            onChange={(e) => change(setTo, e.target.value)}
          />
          {(search || category || status || from || to) && (
            <button
              className="text-button"
              onClick={() => {
                setSearch('');
                setCategory('');
                setStatus('');
                setFrom('');
                setTo('');
                setPage(1);
                setSelected(new Set());
                setLastCsv(null);
              }}
            >
              Clear filters
            </button>
          )}
          <span className="result-count">{filtered.length} expenses</span>
        </div>
        {selected.size > 0 && (
          <div className="selection-bar">
            <span>{selected.size} selected</span>
            <button
              className="button primary small"
              disabled={csvBusy}
              onClick={() => void exportSelected()}
            >
              <FileSpreadsheet size={14} />
              {csvBusy ? 'Exporting…' : 'Export CSV'}
            </button>
            {lastCsv && (
              <span className="inline-export-note">
                Saved {lastCsv.fileName} ·{' '}
                <button
                  className="text-button"
                  onClick={() =>
                    void api.openCsvExport(lastCsv.id).catch((e) => notify(errorMessage(e), true))
                  }
                >
                  Open CSV
                </button>
              </span>
            )}
            <button className="text-button" onClick={clearSelection}>
              Clear selection
            </button>
          </div>
        )}
        {filtered.length ? (
          <ExpenseTable
            expenses={visible}
            selection={{ selected, onToggle: toggleOne, onToggleAll: toggleAllVisible }}
          />
        ) : (
          <div className="empty-state">
            <Search size={26} />
            <h3>{expenses.length ? 'No matching expenses' : 'Your expense register is ready'}</h3>
            <p>
              {expenses.length
                ? 'Try another search or clear your filters.'
                : 'Import your first receipt or add an expense manually.'}
            </p>
          </div>
        )}
        <div className="pagination">
          <span>
            {filtered.length ? (currentPage - 1) * 25 + 1 : 0}–
            {Math.min(currentPage * 25, filtered.length)} of {filtered.length} expenses
          </span>
          <div>
            <button
              className="icon-button"
              disabled={currentPage <= 1}
              onClick={() => setPage((p) => p - 1)}
              aria-label="Previous page"
            >
              <ChevronLeft size={16} />
            </button>
            <span>
              Page {currentPage} of {pages}
            </span>
            <button
              className="icon-button"
              disabled={currentPage >= pages}
              onClick={() => setPage((p) => p + 1)}
              aria-label="Next page"
            >
              <ChevronRight size={16} />
            </button>
          </div>
        </div>
      </section>
    </>
  );
}
