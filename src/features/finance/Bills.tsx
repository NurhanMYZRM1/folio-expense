import { useCallback, useState } from 'react';
import type { Bill, BillInput, BillOccurrence, PaymentMethod } from '../../bindings/generated';
import { api, localToday } from '../../lib/ipc';
import { moneyInput, formatMoney } from '../../lib/money';
import { EmptyState, Loading, Panel } from '../../components/ui';
import {
  AccountOptions,
  ConfirmDelete,
  Field,
  FinanceDialog,
  FinanceFrame,
  MissingAccounts,
  QueryError,
  requiredMoney,
  useFinance,
  useQuery,
} from './shared';
function BillEditor({ bill, close }: { bill?: Bill; close: () => void }) {
  const { workspace, accounts, categories, refreshFinance } = useFinance();
  const [form, setForm] = useState({
    name: bill?.name ?? '',
    kind: bill?.kind ?? 'bill',
    amount: bill ? moneyInput(bill.amountMinor, bill.currency) : '',
    account: bill?.accountId ?? accounts.find((a) => !a.archivedAt)?.id ?? '',
    category: bill?.categoryId ?? '',
    payee: bill?.payee ?? '',
    note: bill?.note ?? '',
    frequency: bill?.frequency ?? 'monthly',
    interval: bill?.intervalCount ?? 1,
    anchor: bill?.anchorOn ?? localToday(),
    end: bill?.endsOn ?? '',
    reminderDays: bill?.remindDaysBefore ?? 3,
    reminders: bill?.remindersEnabled ?? true,
    cancelled: bill?.cancelledOn ?? '',
  });
  const update = <K extends keyof typeof form>(key: K, value: (typeof form)[K]) =>
    setForm((v) => ({ ...v, [key]: value }));
  const currency = accounts.find((a) => a.id === form.account)?.currency ?? workspace!.baseCurrency;
  return (
    <FinanceDialog
      title={bill ? 'Edit bill schedule' : 'New bill or subscription'}
      close={close}
      save={async () => {
        const input: BillInput = {
          id: bill?.id ?? null,
          version: bill?.version ?? null,
          workspaceId: workspace!.id,
          name: form.name.trim(),
          kind: form.kind,
          amountMinor: requiredMoney(form.amount, currency),
          accountId: form.account,
          categoryId: form.category || null,
          payee: form.payee || null,
          note: form.note,
          frequency: form.frequency,
          intervalCount: form.interval,
          anchorOn: form.anchor,
          endsOn: form.end || null,
          remindDaysBefore: form.reminderDays,
          remindersEnabled: form.reminders,
          cancelledOn: form.cancelled || null,
        };
        await api.saveBill(input);
        await refreshFinance();
      }}
    >
      <Field label="Bill name">
        <input
          required
          autoFocus
          maxLength={120}
          value={form.name}
          onChange={(e) => update('name', e.target.value)}
        />
      </Field>
      <Field label="Bill type">
        <select value={form.kind} onChange={(e) => update('kind', e.target.value as Bill['kind'])}>
          <option value="bill">Bill</option>
          <option value="subscription">Subscription</option>
        </select>
      </Field>
      <Field label="Paying account">
        <select required value={form.account} onChange={(e) => update('account', e.target.value)}>
          <AccountOptions accounts={accounts} />
        </select>
      </Field>
      <Field label={`Expected amount (${currency})`}>
        <input
          required
          inputMode="decimal"
          value={form.amount}
          onChange={(e) => update('amount', e.target.value)}
        />
      </Field>
      <div className="form-grid">
        <Field label="Recurrence">
          <select
            value={form.frequency}
            onChange={(e) => update('frequency', e.target.value as Bill['frequency'])}
          >
            {['once', 'weekly', 'monthly', 'yearly'].map((f) => (
              <option key={f}>{f}</option>
            ))}
          </select>
        </Field>
        <Field label="Every (periods)">
          <input
            type="number"
            min={1}
            max={52}
            required
            value={form.interval}
            onChange={(e) => update('interval', Number(e.target.value))}
          />
        </Field>
      </div>
      <div className="form-grid">
        <Field label="Next scheduled due date">
          <input
            type="date"
            required
            value={form.anchor}
            onChange={(e) => update('anchor', e.target.value)}
          />
        </Field>
        <Field label="Schedule ends on">
          <input type="date" value={form.end} onChange={(e) => update('end', e.target.value)} />
        </Field>
      </div>
      {bill && (
        <p className="notice-box">
          Saving starts the schedule at the chosen due date. Past unpaid occurrences of the previous
          schedule are removed; payment history is kept.
        </p>
      )}
      <Field label="Bill category">
        <select value={form.category} onChange={(e) => update('category', e.target.value)}>
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
      <Field label="Bill payee">
        <input
          maxLength={300}
          value={form.payee}
          onChange={(e) => update('payee', e.target.value)}
        />
      </Field>
      <Field label="Bill notes">
        <textarea
          rows={2}
          maxLength={2000}
          value={form.note}
          onChange={(e) => update('note', e.target.value)}
        />
      </Field>
      <label className="check-field">
        <input
          type="checkbox"
          checked={form.reminders}
          onChange={(e) => update('reminders', e.target.checked)}
        />{' '}
        Enable reminders
      </label>
      <Field label="Remind days before">
        <input
          type="number"
          min={0}
          max={60}
          value={form.reminderDays}
          onChange={(e) => update('reminderDays', Number(e.target.value))}
        />
      </Field>
      {bill && (
        <Field label="Cancel after date">
          <input
            type="date"
            value={form.cancelled}
            onChange={(e) => update('cancelled', e.target.value)}
          />
        </Field>
      )}
    </FinanceDialog>
  );
}
function PayBill({ occurrence, close }: { occurrence: BillOccurrence; close: () => void }) {
  const { accounts, refreshFinance } = useFinance();
  const [amount, setAmount] = useState(moneyInput(occurrence.amountMinor, occurrence.currency)),
    [date, setDate] = useState(localToday()),
    [method, setMethod] = useState<PaymentMethod | ''>(''),
    [accountId, setAccount] = useState('');
  return (
    <FinanceDialog
      title={`Record payment: ${occurrence.billName}`}
      close={close}
      label="Record payment"
      save={async () => {
        await api.payBill({
          billId: occurrence.billId,
          dueOn: occurrence.dueOn,
          amountMinor: requiredMoney(amount, occurrence.currency),
          paidOn: date,
          accountId: accountId || null,
          paymentMethod: method || null,
          note: '',
        });
        await refreshFinance();
      }}
    >
      <p>
        Due {occurrence.dueOn}. Recording the payment creates one expense. Repeating this action
        keeps the original payment.
      </p>
      <Field label={`Paid amount (${occurrence.currency})`}>
        <input
          required
          inputMode="decimal"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
      </Field>
      <Field label="Paid on">
        <input required type="date" value={date} onChange={(e) => setDate(e.target.value)} />
      </Field>
      <Field label="Payment account">
        <select value={accountId} onChange={(e) => setAccount(e.target.value)}>
          <option value="">Use bill's account</option>
          <AccountOptions accounts={accounts.filter((a) => a.currency === occurrence.currency)} />
        </select>
      </Field>
      <Field label="Payment method">
        <select value={method} onChange={(e) => setMethod(e.target.value as PaymentMethod | '')}>
          <option value="">Not specified</option>
          <option value="bank_transfer">Bank transfer</option>
          <option value="cash">Cash</option>
          <option value="credit_card">Credit card</option>
          <option value="ewallet">E-wallet</option>
          <option value="duitnow">DuitNow</option>
        </select>
      </Field>
    </FinanceDialog>
  );
}
export function Bills() {
  const { workspace, accounts, refreshFinance } = useFinance();
  const [edit, setEdit] = useState<Bill | 'new' | null>(null),
    [remove, setRemove] = useState<Bill | null>(null),
    [pay, setPay] = useState<BillOccurrence | null>(null),
    [detail, setDetail] = useState<Bill | null>(null),
    [permission, setPermission] = useState(''),
    [requesting, setRequesting] = useState(false);
  const native = (
    window as unknown as {
      __folio?: { invoke: (command: string, args: string) => Promise<string> };
    }
  ).__folio;
  async function enableReminders() {
    if (!native) return;
    setRequesting(true);
    try {
      const response = JSON.parse(await native.invoke('request_reminders', '{}')) as {
        result?: boolean;
        error?: { message: string };
      };
      if (response.error) throw new Error(response.error.message);
      setPermission(
        response.result
          ? 'Notifications enabled. Bill reminders are scheduled at 09:00 local time.'
          : 'Notification permission denied. Enable notifications for Folio in your device Settings to receive reminders.',
      );
    } catch (e) {
      setPermission(
        e instanceof Error ? e.message : 'Notification permission could not be requested.',
      );
    } finally {
      setRequesting(false);
    }
  }
  const today = localToday();
  const query = useQuery(
    useCallback(() => (workspace ? api.bills(workspace.id) : Promise.resolve([])), [workspace]),
  );
  const occurrences = useQuery(
    useCallback(
      () => (workspace ? api.billOccurrences(workspace.id, today, 90) : Promise.resolve([])),
      [workspace, today],
    ),
  );
  return (
    <FinanceFrame
      title="Bills & subscriptions"
      subtitle="See what is due, track payments and manage recurring expenses."
      actions={
        <button
          className="button primary"
          disabled={!accounts.some((a) => !a.archivedAt)}
          onClick={() => setEdit('new')}
        >
          New bill
        </button>
      }
    >
      {native && (
        <div className="notice-box">
          <div>
            <p>
              Bill reminders need notification permission on this device. Manage bill and budget
              alerts in Settings.
            </p>
            <button
              type="button"
              className="button secondary"
              disabled={requesting}
              onClick={() => void enableReminders()}
            >
              {requesting ? 'Requesting permission…' : 'Enable reminder notifications'}
            </button>
            {permission && <p role="status">{permission}</p>}
          </div>
        </div>
      )}
      {!accounts.length ? (
        <MissingAccounts />
      ) : (
        <>
          <QueryError error={query.error || occurrences.error} />
          <Panel title="Upcoming & overdue · next 90 days">
            {occurrences.loading ? (
              <Loading />
            ) : occurrences.data?.length ? (
              occurrences.data.map((o) => (
                <div key={`${o.billId}:${o.dueOn}`} className="finance-list-row">
                  <div>
                    <strong>{o.billName}</strong>
                    <small>
                      {o.dueOn} · {o.kind}
                    </small>
                  </div>
                  <span className={`badge ${o.status === 'overdue' ? 'failed' : ''}`}>
                    {o.status.replaceAll('_', ' ')}
                  </span>
                  <strong>{formatMoney(o.amountMinor, o.currency)}</strong>
                  {o.status !== 'paid' && (
                    <button className="button secondary" onClick={() => setPay(o)}>
                      Pay {o.billName}
                    </button>
                  )}
                </div>
              ))
            ) : (
              <EmptyState
                title="No bills due"
                description="Your scheduled bills and subscriptions will appear here."
              />
            )}
          </Panel>
          <Panel title="Your schedules">
            {query.data?.length ? (
              query.data.map((b) => (
                <div key={b.id} className="finance-list-row">
                  <div>
                    <strong>{b.name}</strong>
                    <small>
                      {b.frequency} ·{' '}
                      {b.cancelledOn
                        ? `cancelled after ${b.cancelledOn}`
                        : b.nextDueOn
                          ? `next due ${b.nextDueOn}`
                          : 'no remaining occurrences'}
                    </small>
                  </div>
                  <strong>{formatMoney(b.amountMinor, b.currency)}</strong>
                  <button className="text-button" onClick={() => setDetail(b)}>
                    History
                  </button>
                  <button className="text-button" onClick={() => setEdit(b)}>
                    Edit {b.name}
                  </button>
                  <button className="text-button danger-text" onClick={() => setRemove(b)}>
                    Delete
                  </button>
                </div>
              ))
            ) : (
              <p className="finance-panel-copy">
                No bill schedules yet. Create a bill to track upcoming payments.
              </p>
            )}
          </Panel>
        </>
      )}
      {edit && <BillEditor bill={edit === 'new' ? undefined : edit} close={() => setEdit(null)} />}
      {pay && <PayBill occurrence={pay} close={() => setPay(null)} />}
      {detail && <BillHistory bill={detail} close={() => setDetail(null)} />}
      {remove && (
        <ConfirmDelete
          title={`Delete ${remove.name}?`}
          description="The schedule and future reminders are removed. Recorded payments remain in transaction history."
          close={() => setRemove(null)}
          remove={async () => {
            await api.deleteBill(remove.id);
            await refreshFinance();
          }}
        />
      )}
    </FinanceFrame>
  );
}
function BillHistory({ bill, close }: { bill: Bill; close: () => void }) {
  const query = useQuery(useCallback(() => api.bill(bill.id, localToday()), [bill.id]));
  return (
    <FinanceDialog
      title={`${bill.name} payment history`}
      close={close}
      save={async () => {}}
      label="Done"
    >
      <QueryError error={query.error} />
      {query.loading ? (
        <Loading />
      ) : (
        query.data?.occurrences.map((o) => (
          <div className="finance-list-row" key={o.dueOn}>
            <span>{o.dueOn}</span>
            <span>{o.status.replaceAll('_', ' ')}</span>
            <strong>{formatMoney(o.paidAmountMinor ?? o.amountMinor, o.currency)}</strong>
          </div>
        ))
      )}
    </FinanceDialog>
  );
}
