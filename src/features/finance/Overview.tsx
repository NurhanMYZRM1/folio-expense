import { useCallback, useState } from 'react';
import { Link } from 'react-router-dom';
import { api, localToday } from '../../lib/ipc';
import { formatMoney } from '../../lib/money';
import { EmptyState, Loading, Panel, TextLink } from '../../components/ui';
import { FinanceFrame, MonthPicker, QueryError, signedMoney, useFinance, useQuery } from './shared';
import { TransactionRows } from './Transactions';
export function FinanceOverview() {
  const { workspace } = useFinance();
  const today = localToday();
  const query = useQuery(
    useCallback(
      () => (workspace ? api.overview(workspace.id, today) : Promise.resolve(null)),
      [workspace, today],
    ),
  );
  const data = query.data;
  return (
    <FinanceFrame
      title="Finance overview"
      subtitle="Your accounts, spending plans and upcoming payments in one place."
      actions={
        <Link className="button primary" to="/finance/transactions?new">
          Record transaction
        </Link>
      }
    >
      <QueryError error={query.error} />
      {query.loading ? (
        <Loading />
      ) : data ? (
        <>
          <div className="metrics-grid finance-metrics">
            {[
              ['Account balance', data.totalBalanceMinor],
              ['Income this month', data.monthIncomeMinor],
              ['Spending this month', data.monthExpenseMinor],
            ].map(([label, amount]) => (
              <section className="metric" key={label}>
                <span className="metric-top">{label}</span>
                <strong className="metric-value">
                  {signedMoney(amount as number, data.workspace.baseCurrency)}
                </strong>
                <span className="muted">
                  {label === 'Account balance'
                    ? 'Loans excluded · base-currency accounts only'
                    : today.slice(0, 7)}
                </span>
              </section>
            ))}
          </div>
          {data.otherBalances.length > 0 && (
            <p className="notice-box">
              Other currencies, shown separately:{' '}
              {data.otherBalances.map((b) => signedMoney(b.amountMinor, b.currency)).join(' · ')}.
              Balances are not converted.
            </p>
          )}
          {!data.accounts.length && (
            <EmptyState
              title="Start with an account"
              description="Add the bank account, cash or e-wallet you want to track."
              action={
                <Link className="button primary" to="/finance/accounts">
                  Add an account
                </Link>
              }
            />
          )}
          <div className="analytics-grid">
            <Panel
              title="Monthly budgets"
              action={<TextLink to="/finance/budgets">Manage budgets</TextLink>}
            >
              {data.budgets.length ? (
                data.budgets.map((b) => (
                  <div className="finance-list-row" key={b.budget.id}>
                    <div>
                      <strong>{b.budget.categoryName ?? 'All spending'}</strong>
                      <small>{signedMoney(b.remainingMinor, b.baseCurrency)} remaining</small>
                      <progress
                        aria-label={`${b.budget.categoryName ?? 'All spending'} progress`}
                        value={Math.max(0, b.spentMinor)}
                        max={b.budget.amountMinor}
                      />
                    </div>
                    <span>{b.percent}%</span>
                  </div>
                ))
              ) : (
                <p className="finance-panel-copy">No monthly budgets yet.</p>
              )}
            </Panel>
            <Panel
              title="Bills due soon"
              action={<TextLink to="/finance/bills">View bills</TextLink>}
            >
              {data.bills.length ? (
                data.bills.map((b) => (
                  <div className="finance-list-row" key={`${b.billId}:${b.dueOn}`}>
                    <div>
                      <strong>{b.billName}</strong>
                      <small>
                        {b.dueOn} · {b.status.replaceAll('_', ' ')}
                      </small>
                    </div>
                    <strong>{formatMoney(b.amountMinor, b.currency)}</strong>
                  </div>
                ))
              ) : (
                <p className="finance-panel-copy">No unpaid bills due in the next 14 days.</p>
              )}
            </Panel>
          </div>
          <Panel
            title="Recent transactions"
            action={<TextLink to="/finance/transactions">All transactions</TextLink>}
          >
            <TransactionRows items={data.recent} />
          </Panel>
          <p className="finance-panel-copy">
            Corporate expense claims stay in <Link to="/expenses">Receipts & expenses</Link>. Record
            a reviewed receipt into an account explicitly to include it in your finances.
          </p>
        </>
      ) : null}
    </FinanceFrame>
  );
}
export function Reports() {
  const { workspace } = useFinance();
  const [period, setPeriod] = useState(localToday().slice(0, 7));
  const query = useQuery(
    useCallback(
      () => (workspace ? api.monthlyReport(workspace.id, period) : Promise.resolve(null)),
      [workspace, period],
    ),
  );
  const data = query.data;
  return (
    <FinanceFrame
      title="Monthly reports"
      subtitle="Income and spending in your reporting currency. Transfers stay separate."
    >
      <MonthPicker value={period} set={setPeriod} />
      <QueryError error={query.error} />
      {query.loading ? (
        <Loading />
      ) : data ? (
        <>
          <div className="metrics-grid finance-metrics">
            {[
              ['Income', data.incomeMinor],
              ['Spending', data.expenseMinor],
              ['Net', data.netMinor],
            ].map(([label, amount]) => (
              <section className="metric" key={label}>
                <span className="metric-top">{label}</span>
                <strong className="metric-value">
                  {signedMoney(amount as number, data.baseCurrency)}
                </strong>
              </section>
            ))}
          </div>
          <p className="finance-panel-copy">
            Previous month: income {formatMoney(data.previousIncomeMinor, data.baseCurrency)} ·
            spending {formatMoney(data.previousExpenseMinor, data.baseCurrency)}.
          </p>
          {data.unconvertedCount > 0 && (
            <div className="notice-box">
              {data.unconvertedCount} transactions are awaiting an exchange rate and are excluded
              from these totals.
            </div>
          )}
          <Panel title="By category">
            {data.byCategory.length ? (
              <div className="table-wrap">
                <table className="finance-table">
                  <thead>
                    <tr>
                      <th>Category</th>
                      <th>Kind</th>
                      <th>Transactions</th>
                      <th>Total</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.byCategory.map((c) => (
                      <tr key={`${c.kind}:${c.categoryId}`}>
                        <td>{c.name}</td>
                        <td>{c.kind}</td>
                        <td>{c.count}</td>
                        <td>{formatMoney(c.totalMinor, data.baseCurrency)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <EmptyState
                title="No activity this month"
                description="Transactions you record in this calendar month will appear here."
              />
            )}
          </Panel>
          <Panel title="Daily activity">
            <div className="table-wrap">
              <table className="finance-table">
                <thead>
                  <tr>
                    <th>Date</th>
                    <th>Income</th>
                    <th>Spending</th>
                  </tr>
                </thead>
                <tbody>
                  {data.daily.map((d) => (
                    <tr key={d.date}>
                      <td>{d.date}</td>
                      <td>{formatMoney(d.incomeMinor, data.baseCurrency)}</td>
                      <td>{formatMoney(d.expenseMinor, data.baseCurrency)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
        </>
      ) : null}
    </FinanceFrame>
  );
}
