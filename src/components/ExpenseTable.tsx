import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { ChevronRight, FileText } from 'lucide-react';
import type { Expense } from '../bindings/generated';
import { dateLabel } from '../lib/constants';
import { formatMoney } from '../lib/money';
import { StatusBadge, EmptyState } from './ui';
import { api } from '../lib/ipc';
function Thumbnail({ id }: { id: string }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void api
      .thumbnail(id)
      .then((v) => {
        if (live) setSrc(v);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [id]);
  return src ? (
    <img className="table-thumbnail" src={src} alt="Receipt thumbnail" loading="lazy" />
  ) : (
    <FileText size={16} />
  );
}
export type ExpenseSelection = {
  selected: Set<string>;
  onToggle: (id: string) => void;
  onToggleAll: (checked: boolean) => void;
};
export function ExpenseTable({
  expenses,
  compact = false,
  actions,
  selection,
}: {
  expenses: Expense[];
  compact?: boolean;
  actions?: (e: Expense) => React.ReactNode;
  selection?: ExpenseSelection;
}) {
  if (!expenses.length)
    return (
      <EmptyState
        title="No expenses here yet"
        description="Import a receipt to start building your local expense record."
        action={
          <Link className="button secondary" to="/import">
            Import a receipt
          </Link>
        }
      />
    );
  const allVisibleSelected = !!selection && expenses.every((e) => selection.selected.has(e.id));
  const someVisibleSelected = !!selection && expenses.some((e) => selection.selected.has(e.id));
  return (
    <div className="table-wrap">
      <table className="data-table">
        <thead>
          <tr>
            {selection && (
              <th className="checkbox-cell">
                <input
                  type="checkbox"
                  aria-label="Select all visible expenses"
                  checked={allVisibleSelected}
                  ref={(el) => {
                    if (el) el.indeterminate = someVisibleSelected && !allVisibleSelected;
                  }}
                  onChange={(e) => selection.onToggleAll(e.target.checked)}
                />
              </th>
            )}
            <th>Date</th>
            <th>Merchant / expense</th>
            <th>Category</th>
            <th className="numeric">Amount</th>
            <th>Status</th>
            {!compact && <th>Receipt</th>}
            <th>
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {expenses.map((e) => (
            <tr key={e.id}>
              {selection && (
                <td className="checkbox-cell">
                  <input
                    type="checkbox"
                    aria-label={`Select ${e.merchantName || 'expense'}`}
                    checked={selection.selected.has(e.id)}
                    onChange={() => selection.onToggle(e.id)}
                  />
                </td>
              )}
              <td className="date-cell">{dateLabel(e.occurredAt)}</td>
              <td>
                <Link className="merchant" to={`/expenses/${e.id}`}>
                  <span className="merchant-avatar">
                    {(e.merchantName || e.receiptFilename || 'E').slice(0, 1).toUpperCase()}
                  </span>
                  <span>
                    <strong>{e.merchantName || 'Untitled expense'}</strong>
                    <small>{e.description || e.receiptFilename || 'Manual entry'}</small>
                  </span>
                </Link>
              </td>
              <td>
                <span className="category-label">{e.category}</span>
              </td>
              <td className="numeric amount">
                {formatMoney(e.totalAmountMinor, e.currency ?? 'MYR')}
                {e.originalCurrency && (
                  <span className="original-amount">
                    {formatMoney(e.originalTotalAmountMinor, e.originalCurrency)}
                  </span>
                )}
              </td>
              <td>
                <StatusBadge status={e.status} />
              </td>
              {!compact && (
                <td>
                  {e.receiptId ? (
                    <Link
                      className="receipt-indicator"
                      to={`/expenses/${e.id}`}
                      aria-label={`View receipt for ${e.merchantName || 'expense'}`}
                    >
                      <Thumbnail id={e.receiptId} />
                      <span>Attached</span>
                    </Link>
                  ) : (
                    <span className="muted">—</span>
                  )}
                </td>
              )}
              <td>
                {actions ? (
                  actions(e)
                ) : (
                  <Link
                    to={`/expenses/${e.id}`}
                    className="icon-button"
                    aria-label={`Open ${e.merchantName || 'expense'}`}
                  >
                    <ChevronRight size={16} />
                  </Link>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
