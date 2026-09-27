import { useState } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { Search, Upload, Plus, ChevronLeft, ChevronRight, SlidersHorizontal } from 'lucide-react';
import { useWorkspace } from '../../app/providers';
import { PageHeader } from '../../components/ui';
import { ExpenseTable } from '../../components/ExpenseTable';
import { CATEGORIES, STATUS_LABELS } from '../../lib/constants';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
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
  function change(fn: (v: string) => void, value: string) {
    fn(value);
    setPage(1);
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
              }}
            >
              Clear filters
            </button>
          )}
          <span className="result-count">{filtered.length} expenses</span>
        </div>
        {filtered.length ? (
          <ExpenseTable expenses={filtered.slice((currentPage - 1) * 25, currentPage * 25)} />
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
