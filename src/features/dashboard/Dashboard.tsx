import { useState } from 'react';
import { Link } from 'react-router-dom';
import {
  Upload,
  ArrowUpRight,
  Wallet,
  CircleDashed,
  ScanLine,
  Files,
  ArrowRight,
  CalendarDays,
  ShieldCheck,
} from 'lucide-react';
import { useWorkspace } from '../../app/providers';
import { PageHeader, Panel, TextLink, StatusBadge } from '../../components/ui';
import { ExpenseTable } from '../../components/ExpenseTable';
import { CURRENCIES, formatMoney, sumMinor } from '../../lib/money';
import { CATEGORIES, CATEGORY_COLORS, dateLabel } from '../../lib/constants';
export function Dashboard() {
  const { expenses, claims, settings } = useWorkspace();
  const [currency, setCurrency] = useState(settings.defaultCurrency);
  const today = new Date();
  const currentMonth = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}`;
  const relevant = expenses.filter((e) => e.currency === currency && e.status !== 'archived');
  const monthly = relevant.filter((e) => e.occurredAt?.startsWith(currentMonth));
  const monthTotal = sumMinor(monthly.map((e) => e.totalAmountMinor));
  const unsubmitted = relevant.filter((e) => !['submitted', 'archived'].includes(e.status));
  const review = expenses.filter((e) => ['needs_review', 'draft'].includes(e.status));
  const months = Array.from({ length: 6 }, (_, index) => {
    const d = new Date(today.getFullYear(), today.getMonth() - 5 + index, 1);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
    return {
      label: d.toLocaleDateString('en', { month: 'short' }),
      value: sumMinor(
        relevant.filter((e) => e.occurredAt?.startsWith(key)).map((e) => e.totalAmountMinor),
      ),
    };
  });
  const max = Math.max(1, ...months.map((m) => m.value));
  const categories = CATEGORIES.map((name, i) => ({
    name,
    color: CATEGORY_COLORS[i],
    value: sumMinor(monthly.filter((e) => e.category === name).map((e) => e.totalAmountMinor)),
  }))
    .filter((c) => c.value > 0)
    .sort((a, b) => b.value - a.value);
  const cards = [
    {
      label: 'Expenses this month',
      value: formatMoney(monthTotal, currency),
      icon: Wallet,
      note: `${monthly.length} expenses recorded`,
      tag: 'This month',
    },
    {
      label: 'Unsubmitted expenses',
      value: formatMoney(sumMinor(unsubmitted.map((e) => e.totalAmountMinor)), currency),
      icon: CircleDashed,
      note: `${unsubmitted.length} expenses to claim`,
      tag: 'Unsubmitted',
    },
    {
      label: 'Receipts to review',
      value: String(review.length).padStart(2, '0'),
      icon: ScanLine,
      note: review.length ? 'A quick check before you claim' : 'Everything is up to date',
      tag: 'Action needed',
    },
    {
      label: 'Claim packages',
      value: String(claims.length).padStart(2, '0'),
      icon: Files,
      note: `${claims.filter((c) => c.status === 'draft').length} drafts in your workspace`,
      tag: 'All time',
    },
  ];
  return (
    <>
      <PageHeader
        eyebrow="YOUR WORKSPACE, AT A GLANCE"
        title="Expense overview"
        subtitle="A clear view of your spending, receipts, and claims."
        actions={
          <>
            <select
              className="currency-select"
              aria-label="Dashboard currency"
              value={currency}
              onChange={(e) => setCurrency(e.target.value)}
            >
              {CURRENCIES.map((c) => (
                <option key={c}>{c}</option>
              ))}
            </select>
            <Link to="/import" className="button primary">
              <Upload size={16} />
              Import receipts
            </Link>
          </>
        }
      />
      <div className="period-row">
        <span>
          <CalendarDays size={14} />
          {today.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' })}
        </span>
        <span>All totals in {currency} · no currency conversion</span>
      </div>
      <div className="metrics-grid">
        {cards.map((c, i) => (
          <section className={`metric metric-${i}`} key={c.label}>
            <div className="metric-top">
              <span>{c.label}</span>
              <c.icon size={17} />
            </div>
            <strong className="metric-value">{c.value}</strong>
            <div className="metric-bottom">
              <span>{c.note}</span>
              {i === 2 && review.length > 0 && (
                <Link to="/expenses?status=needs_review" aria-label="Review expenses">
                  <ArrowUpRight size={16} />
                </Link>
              )}
            </div>
          </section>
        ))}
      </div>
      {review.length > 0 ? (
        <Link className="review-strip" to="/expenses?status=needs_review">
          <span className="review-strip-icon">
            <ScanLine size={18} />
          </span>
          <span>
            <strong>
              {review.length} {review.length === 1 ? 'receipt needs' : 'receipts need'} a second
              look
            </strong>
            <span>Check the highlighted fields to make your expenses claim-ready.</span>
          </span>
          <span className="review-link">
            Review receipts <ArrowRight size={16} />
          </span>
        </Link>
      ) : (
        <div className="review-strip calm">
          <span className="review-strip-icon">
            <ShieldCheck size={18} />
          </span>
          <span>
            <strong>A workspace that works offline</strong>
            <span>
              Import receipts, review expenses, and prepare claims. Everything is saved on this
              device.
            </span>
          </span>
          <span className="local-pill">LOCAL-FIRST</span>
        </div>
      )}
      <div className="analytics-grid">
        <Panel
          title="Spending activity"
          action={<span className="panel-meta">Last 6 months · {currency}</span>}
        >
          <div className="chart-summary">
            <strong>{formatMoney(sumMinor(months.map((m) => m.value)), currency)}</strong>
            <span>total recorded spending</span>
          </div>
          <div
            className="bar-chart"
            role="img"
            aria-label={`Spending over six months: ${months.map((m) => `${m.label} ${formatMoney(m.value, currency)}`).join(', ')}`}
          >
            <div className="chart-grid-lines">
              <i />
              <i />
              <i />
            </div>
            {months.map((m, i) => (
              <div className="bar-column" key={m.label}>
                <div
                  className={`chart-bar ${i === 5 ? 'current' : ''}`}
                  style={{ height: `${m.value ? Math.max(4, (m.value / max) * 100) : 2}%` }}
                  title={formatMoney(m.value, currency)}
                />
                <span>{m.label}</span>
              </div>
            ))}
          </div>
        </Panel>
        <Panel title="Expenses by category" action={<span className="panel-meta">This month</span>}>
          {categories.length ? (
            <>
              <div className="category-stack">
                {categories.map((c) => (
                  <span
                    key={c.name}
                    title={c.name}
                    style={{ width: `${(c.value / monthTotal) * 100}%`, background: c.color }}
                  />
                ))}
              </div>
              <div className="category-breakdown">
                {categories.slice(0, 5).map((c) => (
                  <div key={c.name}>
                    <span>
                      <i style={{ background: c.color }} />
                      {c.name}
                    </span>
                    <strong>{formatMoney(c.value, currency)}</strong>
                    <small>{Math.round((c.value / monthTotal) * 100)}%</small>
                  </div>
                ))}
              </div>
            </>
          ) : (
            <div className="category-empty">
              <div className="empty-donut">
                <span>
                  0<span>categories</span>
                </span>
              </div>
              <p>
                Your spending breakdown will
                <br />
                appear as you add expenses.
              </p>
            </div>
          )}
        </Panel>
      </div>
      <Panel title="Recent expenses" action={<TextLink to="/expenses">View all expenses</TextLink>}>
        <ExpenseTable expenses={expenses.slice(0, 5)} compact />
      </Panel>
      <Panel
        title="Recent claims"
        className="recent-claims-panel"
        action={<TextLink to="/claims">View all claims</TextLink>}
      >
        {claims.length ? (
          <div className="recent-claims">
            {claims.slice(0, 3).map((c) => (
              <Link to={`/claims/${c.id}`} key={c.id}>
                <span className="claim-icon">
                  <Files size={20} />
                </span>
                <div>
                  <strong>{c.title}</strong>
                  <span>
                    {c.claimNumber} · {dateLabel(c.createdAt)}
                  </span>
                </div>
                <StatusBadge status={c.status} />
                <b>{formatMoney(c.totalAmountMinor, c.currency)}</b>
                <ArrowUpRight size={16} />
              </Link>
            ))}
          </div>
        ) : (
          <div className="claims-empty-row">
            <span className="claim-icon">
              <Files size={21} />
            </span>
            <div>
              <strong>Your next claim starts here</strong>
              <p>Group reviewed expenses into a professional claim report.</p>
            </div>
            <Link className="button secondary" to="/claims">
              Create a claim <ArrowRight size={14} />
            </Link>
          </div>
        )}
      </Panel>
    </>
  );
}
