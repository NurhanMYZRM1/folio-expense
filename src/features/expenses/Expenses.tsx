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
  Trash2,
} from 'lucide-react';
import { useWorkspace } from '../../app/providers';
import { PageHeader } from '../../components/ui';
import { ExpenseTable } from '../../components/ExpenseTable';
import { CATEGORIES, STATUS_LABELS } from '../../lib/constants';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import type { CsvExport } from '../../bindings/generated';
import { AnimatePresence, motion } from 'motion/react';
import { Modal, insertMotion, spring } from '../../components/motion';
function TabUnderline() {
  return (
    <motion.i className="tab-underline" layoutId="expense-tab" transition={spring.indicator} />
  );
}
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
    [xlsxBusy, setXlsxBusy] = useState(false),
    [lastCsv, setLastCsv] = useState<CsvExport | null>(null),
    [lastXlsx, setLastXlsx] = useState<CsvExport | null>(null),
    [confirmDelete, setConfirmDelete] = useState(false),
    [deleting, setDeleting] = useState(false);
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
    setLastXlsx(null);
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
    setLastXlsx(null);
    setConfirmDelete(false);
  }
  async function deleteSelected() {
    setDeleting(true);
    try {
      const count = await api.deleteExpenses([...selected]);
      clearSelection();
      await refresh();
      notify(`Deleted ${count} expense${count === 1 ? '' : 's'} and their receipts.`);
    } catch (e) {
      notify(errorMessage(e), true);
      setConfirmDelete(false);
    } finally {
      setDeleting(false);
    }
  }
  async function exportSelectedCsv() {
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
  async function exportSelectedXlsx() {
    setXlsxBusy(true);
    try {
      const workbook = await api.exportExpensesXlsx([...selected]);
      setLastXlsx(workbook);
      notify(`Saved ${workbook.fileName} · ${workbook.rows} expenses.`);
    } catch (e) {
      notify(errorMessage(e), true);
    } finally {
      setXlsxBusy(false);
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
          {!status && <TabUnderline />}
        </button>
        {['needs_review', 'ready', 'submitted'].map((s) => (
          <button
            className={status === s ? 'active' : ''}
            key={s}
            onClick={() => change(setStatus, s)}
          >
            {STATUS_LABELS[s]} <span>{expenses.filter((e) => e.status === s).length}</span>
            {status === s && <TabUnderline />}
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
                setLastXlsx(null);
              }}
            >
              Clear filters
            </button>
          )}
          <span className="result-count">{filtered.length} expenses</span>
        </div>
        <AnimatePresence initial={false}>
          {selected.size > 0 && (
            <motion.div className="selection-bar" key="selection" {...insertMotion}>
              <span>{selected.size} selected</span>
              <button
                className="button primary small"
                disabled={csvBusy || xlsxBusy}
                onClick={() => void exportSelectedCsv()}
              >
                <FileSpreadsheet size={14} />
                {csvBusy ? 'Exporting CSV…' : 'Export CSV'}
              </button>
              <button
                className="button secondary small"
                disabled={csvBusy || xlsxBusy}
                onClick={() => void exportSelectedXlsx()}
              >
                <FileSpreadsheet size={14} />
                {xlsxBusy ? 'Exporting Excel…' : 'Export Excel'}
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
              {lastXlsx && (
                <span className="inline-export-note">
                  Saved {lastXlsx.fileName} ·{' '}
                  <button
                    className="text-button"
                    onClick={() =>
                      void api
                        .openXlsxExport(lastXlsx.id)
                        .catch((e) => notify(errorMessage(e), true))
                    }
                  >
                    Open Excel
                  </button>
                </span>
              )}
              <button
                className="button secondary small danger-text"
                disabled={deleting}
                onClick={() => setConfirmDelete(true)}
              >
                <Trash2 size={14} />
                Delete
              </button>
              <button className="text-button" onClick={clearSelection}>
                Clear selection
              </button>
            </motion.div>
          )}
        </AnimatePresence>
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
      <Modal
        open={confirmDelete}
        onClose={() => !deleting && setConfirmDelete(false)}
        labelledBy="delete-expenses-title"
        describedBy="delete-expenses-body"
        role="alertdialog"
      >
        <div className="modal-icon">
          <Trash2 size={22} />
        </div>
        <h2 id="delete-expenses-title">
          Delete {selected.size} {selected.size === 1 ? 'expense' : 'expenses'}?
        </h2>
        <p id="delete-expenses-body">
          The {selected.size === 1 ? 'expense and its' : 'expenses and their'} stored receipt files
          will be removed from this device permanently. This cannot be undone.
        </p>
        <div className="modal-actions">
          <button
            className="button secondary"
            data-autofocus
            disabled={deleting}
            onClick={() => setConfirmDelete(false)}
          >
            Cancel
          </button>
          <button
            className="button danger"
            disabled={deleting}
            onClick={() => void deleteSelected()}
          >
            <Trash2 size={15} />
            {deleting ? 'Deleting…' : 'Delete'}
          </button>
        </div>
      </Modal>
    </>
  );
}
