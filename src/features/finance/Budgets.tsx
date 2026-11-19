import { useCallback, useState } from 'react';
import type { Budget } from '../../bindings/generated';
import { api, localToday } from '../../lib/ipc';
import { moneyInput } from '../../lib/money';
import { EmptyState, Loading, Panel } from '../../components/ui';
import {
  ConfirmDelete,
  Field,
  FinanceDialog,
  FinanceFrame,
  MonthPicker,
  QueryError,
  requiredMoney,
  signedMoney,
  useFinance,
  useQuery,
} from './shared';
function BudgetEditor({ budget, close }: { budget?: Budget; close: () => void }) {
  const { workspace, categories, refreshFinance } = useFinance();
  const [amount, setAmount] = useState(
      budget ? moneyInput(budget.amountMinor, workspace!.baseCurrency) : '',
    ),
    [category, setCategory] = useState(budget?.categoryId ?? ''),
    [threshold, setThreshold] = useState(budget?.alertPercent ?? 80),
    [archived, setArchived] = useState(!!budget?.archivedAt);
  return (
    <FinanceDialog
      title={budget ? 'Edit budget' : 'New monthly budget'}
      close={close}
      save={async () => {
        await api.saveBudget({
          id: budget?.id ?? null,
          version: budget?.version ?? null,
          workspaceId: workspace!.id,
          categoryId: category || null,
          amountMinor: requiredMoney(amount, workspace!.baseCurrency),
          alertPercent: threshold,
          archived,
        });
        await refreshFinance();
      }}
    >
      <Field label="Budget category">
        <select value={category} onChange={(e) => setCategory(e.target.value)}>
          <option value="">All spending</option>
          {categories
            .filter((c) => c.kind === 'expense' && !c.archivedAt)
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
        </select>
      </Field>
      <Field label={`Monthly limit (${workspace!.baseCurrency})`}>
        <input
          inputMode="decimal"
          autoFocus
          required
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
      </Field>
      <Field label="Alert at percent">
        <input
          type="number"
          required
          min={1}
          max={100}
          value={threshold}
          onChange={(e) => setThreshold(Number(e.target.value))}
        />
      </Field>
      <p className="muted">
        Each calendar month starts fresh. Transfers are excluded; category budgets and the
        all-spending budget show overlapping views. Alerts fire once per threshold per month.
      </p>
      {budget && (
        <label className="check-field">
          <input
            type="checkbox"
            checked={archived}
            onChange={(e) => setArchived(e.target.checked)}
          />{' '}
          Archive budget
        </label>
      )}
    </FinanceDialog>
  );
}
export function Budgets() {
  const { workspace, refreshFinance } = useFinance();
  const [period, setPeriod] = useState(localToday().slice(0, 7)),
    [edit, setEdit] = useState<Budget | 'new' | null>(null),
    [remove, setRemove] = useState<Budget | null>(null);
  const query = useQuery(
    useCallback(
      () => (workspace ? api.budgets(workspace.id, period) : Promise.resolve([])),
      [workspace, period],
    ),
  );
  return (
    <FinanceFrame
      title="Budgets"
      subtitle="Set monthly limits and see how much you have left to spend."
      actions={
        <button className="button primary" disabled={!workspace} onClick={() => setEdit('new')}>
          New budget
        </button>
      }
    >
      <MonthPicker value={period} set={setPeriod} />
      <QueryError error={query.error} />
      {query.loading ? (
        <Loading />
      ) : query.data?.length ? (
        <div className="finance-card-grid">
          {query.data.map((p) => (
            <Panel
              key={p.budget.id}
              title={p.budget.categoryName ?? 'All spending'}
              action={
                <span className={`badge ${p.status === 'over' ? 'failed' : ''}`}>
                  {p.budget.archivedAt ? 'Archived' : p.status.replaceAll('_', ' ')}
                </span>
              }
            >
              <div className="finance-budget-body">
                <strong className="finance-balance">
                  {signedMoney(p.remainingMinor, p.baseCurrency)}
                </strong>
                <p>
                  {p.remainingMinor < 0 ? 'over the limit' : 'remaining'} · {p.percent}% used
                </p>
                <progress
                  aria-label={`${p.budget.categoryName ?? 'All spending'} budget used`}
                  value={Math.max(0, p.spentMinor)}
                  max={p.budget.amountMinor}
                />
                <p>
                  {signedMoney(p.spentMinor, p.baseCurrency)} spent of{' '}
                  {signedMoney(p.budget.amountMinor, p.baseCurrency)}
                </p>
                {p.unconvertedCount > 0 && (
                  <p className="notice-box">
                    {p.unconvertedCount} expenses awaiting a currency rate are excluded.
                  </p>
                )}
                <div className="finance-row-actions">
                  <button className="text-button" onClick={() => setEdit(p.budget)}>
                    Edit budget
                  </button>
                  <button className="text-button danger-text" onClick={() => setRemove(p.budget)}>
                    Delete budget
                  </button>
                </div>
              </div>
            </Panel>
          ))}
        </div>
      ) : (
        <EmptyState
          title="Make room for what matters"
          description="Set an overall monthly budget or a limit for an expense category."
        />
      )}
      {edit && (
        <BudgetEditor budget={edit === 'new' ? undefined : edit} close={() => setEdit(null)} />
      )}
      {remove && (
        <ConfirmDelete
          title="Delete this budget?"
          description="Transactions are kept. The monthly limit and its alert history will be removed."
          close={() => setRemove(null)}
          remove={async () => {
            await api.deleteBudget(remove.id);
            await refreshFinance();
          }}
        />
      )}
    </FinanceFrame>
  );
}
