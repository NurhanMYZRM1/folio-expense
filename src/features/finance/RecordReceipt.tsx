import { useState } from 'react';
import type { Expense } from '../../bindings/generated';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { useWorkspace } from '../../app/providers';
import { AccountOptions, Field, useFinance } from './shared';
import { Link } from 'react-router-dom';
export function RecordReceipt({ expense, disabled }: { expense: Expense; disabled: boolean }) {
  const { workspace, accounts, categories, refreshFinance } = useFinance();
  const { notify } = useWorkspace();
  const [account, setAccount] = useState(''),
    [category, setCategory] = useState(''),
    [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  const matching = accounts.filter((a) => !a.archivedAt && a.currency === expense.currency);
  async function record() {
    setBusy(true);
    setError('');
    try {
      await api.recordExpense({
        expenseId: expense.id,
        accountId: account,
        categoryId: category || null,
        paymentMethod: null,
        note: expense.description,
      });
      await refreshFinance();
      notify(
        'Receipt recorded in your finance ledger. Repeating this action keeps the original transaction.',
      );
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="finance-receipt-panel">
      <h3>Record in finance</h3>
      <p>
        Include this reviewed expense in {workspace?.name ?? 'your finance workspace'}. Recording it
        does not change its claim status. Amount, date and merchant follow future receipt
        corrections.
      </p>
      {error && <div className="inline-error">{error}</div>}
      {matching.length ? (
        <>
          <Field label="Finance account">
            <select value={account} onChange={(e) => setAccount(e.target.value)}>
              <option value="">Choose a {expense.currency} account</option>
              <AccountOptions accounts={matching} />
            </select>
          </Field>
          <Field label="Finance category">
            <select value={category} onChange={(e) => setCategory(e.target.value)}>
              <option value="">Uncategorised</option>
              {categories
                .filter((c) => c.kind === 'expense' && !c.archivedAt)
                .map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
            </select>
          </Field>
          <button
            type="button"
            className="button secondary"
            disabled={
              disabled ||
              busy ||
              !account ||
              !['ready', 'submitted', 'archived'].includes(expense.status)
            }
            onClick={() => void record()}
          >
            {busy ? 'Recording…' : 'Record reviewed receipt'}
          </button>
          <p className="muted">
            Save changes and mark the receipt as ready first. Deleting this receipt also deletes its
            linked finance transaction.
          </p>
        </>
      ) : (
        <p>
          <Link to="/finance/accounts">Create an account</Link> in{' '}
          {expense.currency ?? 'the receipt currency'} to record this expense.
        </p>
      )}
    </section>
  );
}
