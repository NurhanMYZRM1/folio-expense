import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import type {
  PaymentMethod,
  QuickAddDraft,
  Transaction,
  TransactionFilter,
  TransactionInput,
  TransactionSuggestion,
} from '../../bindings/generated';
import { api, localToday } from '../../lib/ipc';
import { formatMoney, moneyInput } from '../../lib/money';
import { errorMessage } from '../../lib/errors';
import { EmptyState, Loading, Panel } from '../../components/ui';
import { useWorkspace } from '../../app/providers';
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

type EntryDraft = QuickAddDraft & { entrySource?: 'quick_add' | 'voice' };

export function TransactionEditor({
  transaction,
  draft,
  suggestion,
  remove,
  close,
}: {
  transaction?: Transaction;
  draft?: EntryDraft;
  suggestion?: TransactionSuggestion;
  remove?: () => void;
  close: () => void;
}) {
  const { workspace, accounts, categories, refreshFinance } = useFinance();
  const { notify } = useWorkspace();
  const active = accounts.filter((a) => !a.archivedAt);
  const [form, setForm] = useState({
    kind: transaction?.kind ?? suggestion?.kind ?? draft?.kind ?? 'expense',
    account:
      transaction?.accountId ??
      draft?.accountId ??
      active.find((a) => a.currency === (suggestion?.currency ?? draft?.currency))?.id ??
      active[0]?.id ??
      '',
    amount: transaction
      ? moneyInput(transaction.amountMinor, transaction.currency)
      : suggestion
        ? moneyInput(suggestion.amountMinor, suggestion.currency)
        : draft?.amountMinor && draft.currency
          ? moneyInput(draft.amountMinor, draft.currency)
          : '',
    toAccount: transaction?.toAccountId ?? '',
    toAmount:
      transaction?.toAmountMinor && transaction.toCurrency
        ? moneyInput(transaction.toAmountMinor, transaction.toCurrency)
        : '',
    date: transaction?.occurredOn ?? suggestion?.occurredOn ?? draft?.occurredOn ?? localToday(),
    category: transaction?.categoryId ?? draft?.categoryId ?? '',
    payee: transaction?.payee ?? suggestion?.payee ?? draft?.payee ?? '',
    note: transaction?.note ?? draft?.note ?? '',
    method: transaction?.paymentMethod ?? '',
  });
  const update = <K extends keyof typeof form>(key: K, value: (typeof form)[K]) =>
    setForm((v) => ({ ...v, [key]: value }));
  const account = accounts.find((a) => a.id === form.account),
    to = accounts.find((a) => a.id === form.toAccount);
  const linked = !!transaction?.expenseId;
  return (
    <FinanceDialog
      title={
        transaction
          ? 'Edit transaction'
          : suggestion
            ? 'Review payment suggestion'
            : draft
              ? 'Review quick entry'
              : 'New transaction'
      }
      close={close}
      save={async () => {
        if (!account) throw new Error('Choose an account.');
        const amount = requiredMoney(form.amount, account.currency);
        const input: TransactionInput = {
          id: transaction?.id ?? null,
          version: transaction?.version ?? null,
          workspaceId: workspace!.id,
          kind: form.kind,
          accountId: form.account,
          amountMinor: amount,
          toAccountId: form.kind === 'transfer' ? form.toAccount || null : null,
          toAmountMinor:
            form.kind === 'transfer'
              ? to?.currency === account.currency
                ? amount
                : requiredMoney(form.toAmount, to?.currency ?? account.currency)
              : null,
          occurredOn: form.date,
          categoryId: form.kind === 'transfer' ? null : form.category || null,
          payee: form.payee.trim() || null,
          note: form.note,
          paymentMethod: (form.method as PaymentMethod) || null,
          source:
            transaction?.source ??
            (suggestion ? 'notification' : draft ? (draft.entrySource ?? 'quick_add') : 'manual'),
        };
        if (suggestion) await api.acceptSuggestion(suggestion.id, input);
        else await api.saveTransaction(input);
        await refreshFinance();
        notify('Transaction saved on this device.');
      }}
      label={transaction ? 'Save transaction' : 'Confirm transaction'}
    >
      {remove && (
        <button type="button" className="text-button danger-text" onClick={remove}>
          Delete transaction
        </button>
      )}
      {draft?.uncertain.length ? (
        <div className="notice-box">
          Please verify {draft.uncertain.join(', ')} before saving. Nothing has been recorded yet.
        </div>
      ) : null}
      {suggestion && (
        <p className="muted">
          Suggested by {suggestion.appLabel}. Verify the account, amount and merchant before
          confirming.
        </p>
      )}
      {linked && (
        <p className="notice-box">
          The receipt controls the amount, date and payee.{' '}
          <Link to={`/expenses/${transaction!.expenseId}`}>Edit the receipt</Link> to update them.
        </p>
      )}
      {transaction?.billId && (
        <p className="notice-box">
          This transaction records a bill payment. Deleting it makes that occurrence unpaid.
        </p>
      )}
      <Field label="Transaction kind">
        <select
          value={form.kind}
          disabled={linked || !!transaction?.billId}
          onChange={(e) => {
            update('kind', e.target.value as TransactionInput['kind']);
            update('category', '');
          }}
        >
          <option value="expense">Expense</option>
          <option value="income">Income</option>
          <option value="transfer">Transfer</option>
        </select>
      </Field>
      <Field label={form.kind === 'transfer' ? 'From account' : 'Transaction account'}>
        <select required value={form.account} onChange={(e) => update('account', e.target.value)}>
          <AccountOptions accounts={accounts} includeArchived={!!transaction} />
        </select>
      </Field>
      <div className="form-grid">
        <Field label={`Amount (${account?.currency ?? ''})`}>
          <input
            required
            disabled={linked}
            inputMode="decimal"
            value={form.amount}
            onChange={(e) => update('amount', e.target.value)}
          />
        </Field>
        <Field label="Transaction date">
          <input
            required
            disabled={linked}
            type="date"
            value={form.date}
            onChange={(e) => update('date', e.target.value)}
          />
        </Field>
      </div>
      {form.kind === 'transfer' ? (
        <>
          <Field label="To account">
            <select
              required
              value={form.toAccount}
              onChange={(e) => update('toAccount', e.target.value)}
            >
              <option value="">Choose receiving account</option>
              <AccountOptions accounts={active.filter((a) => a.id !== form.account)} />
            </select>
          </Field>
          {to && to.currency !== account?.currency && (
            <Field label={`Received amount (${to.currency})`}>
              <input
                required
                inputMode="decimal"
                value={form.toAmount}
                onChange={(e) => update('toAmount', e.target.value)}
              />
            </Field>
          )}
          <p className="muted">
            Transfers change account balances and stay outside income, spending and budgets. For
            different currencies, enter the actual amount received.
          </p>
        </>
      ) : (
        <Field label="Transaction category">
          <select value={form.category} onChange={(e) => update('category', e.target.value)}>
            <option value="">Uncategorised</option>
            {categories
              .filter((c) => c.kind === form.kind && (!c.archivedAt || c.id === form.category))
              .map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
          </select>
        </Field>
      )}
      <Field label="Payee / payer">
        <input
          disabled={linked}
          maxLength={300}
          value={form.payee}
          onChange={(e) => update('payee', e.target.value)}
        />
      </Field>
      <Field label="Payment method">
        <select
          value={form.method}
          onChange={(e) => update('method', e.target.value as PaymentMethod | '')}
        >
          <option value="">Not specified</option>
          {[
            'cash',
            'debit_card',
            'credit_card',
            'ewallet',
            'bank_transfer',
            'duitnow',
            'fpx',
            'cheque',
            'other',
          ].map((m) => (
            <option key={m} value={m}>
              {m.replaceAll('_', ' ')}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Transaction notes">
        <textarea
          rows={2}
          maxLength={4000}
          value={form.note}
          onChange={(e) => update('note', e.target.value)}
        />
      </Field>
    </FinanceDialog>
  );
}
export function TransactionRows({
  items,
  edit,
}: {
  items: Transaction[];
  edit?: (t: Transaction) => void;
}) {
  return items.length ? (
    <div className="table-wrap">
      <table className="finance-table">
        <thead>
          <tr>
            <th>Date</th>
            <th>Payee / note</th>
            <th>Account</th>
            <th>Category</th>
            <th>Amount</th>
            {edit && (
              <th>
                <span className="sr-only">Actions</span>
              </th>
            )}
          </tr>
        </thead>
        <tbody>
          {items.map((t) => (
            <tr key={t.id}>
              <td>{t.occurredOn}</td>
              <td>
                <strong>{t.payee || (t.kind === 'transfer' ? 'Transfer' : t.kind)}</strong>
                {t.note && <small>{t.note}</small>}
                {t.expenseId && (
                  <Link to={`/expenses/${t.expenseId}`} className="text-link">
                    Receipt
                  </Link>
                )}
              </td>
              <td>
                {t.accountName}
                {t.toAccountName && <small>→ {t.toAccountName}</small>}
              </td>
              <td>{t.categoryName || '—'}</td>
              <td className="finance-amount">
                {t.kind === 'expense' ? '−' : t.kind === 'income' ? '+' : ''}
                {formatMoney(t.amountMinor, t.currency)}
                {t.toCurrency && t.toAmountMinor && (
                  <small>→ {formatMoney(t.toAmountMinor, t.toCurrency)}</small>
                )}
                {t.baseAmountMinor === null && t.kind !== 'transfer' && (
                  <small>Awaiting exchange rate</small>
                )}
              </td>
              {edit && (
                <td>
                  <button
                    className="text-button"
                    onClick={() => edit(t)}
                    aria-label={`Edit ${t.payee || t.kind} on ${t.occurredOn}`}
                  >
                    Edit
                  </button>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  ) : (
    <EmptyState
      title="No transactions yet"
      description="Record income, spending or a transfer to see your account activity here."
    />
  );
}
function QuickEntry({ close, review }: { close: () => void; review: (d: EntryDraft) => void }) {
  const { workspace } = useFinance();
  const [text, setText] = useState(''),
    [listening, setListening] = useState(false),
    [speechError, setSpeechError] = useState(''),
    [source, setSource] = useState<'quick_add' | 'voice'>('quick_add');
  const generation = useRef(0);
  const native = (
    window as unknown as {
      __folio?: { invoke: (command: string, args: string) => Promise<string> };
    }
  ).__folio;
  useEffect(
    () => () => {
      generation.current += 1;
      void native?.invoke('cancel_transcription', '{}');
    },
    [native],
  );
  async function dictate() {
    if (!native) return;
    const current = ++generation.current;
    setListening(true);
    setSpeechError('');
    try {
      const result = JSON.parse(await native.invoke('transcribe_voice', '{}')) as {
        result?: string;
        error?: { message: string };
      };
      if (current !== generation.current) return;
      if (!result.result)
        throw new Error(
          result.error?.message ?? 'No speech was recognised. Try typing your transaction.',
        );
      setText(result.result);
      setSource('voice');
    } catch (e) {
      if (current === generation.current) setSpeechError(errorMessage(e));
    } finally {
      if (current === generation.current) setListening(false);
    }
  }
  function cancelSpeech() {
    generation.current += 1;
    setListening(false);
    void native?.invoke('cancel_transcription', '{}');
  }
  return (
    <FinanceDialog
      title="Quick entry"
      close={close}
      label="Review entry"
      save={async () => {
        const d = await api.parseQuickAdd(workspace!.id, text, localToday());
        if (listening) throw new Error('Finish or cancel dictation before reviewing.');
        review({ ...d, entrySource: source });
      }}
    >
      {native && (
        <div className="finance-row-actions">
          {listening ? (
            <>
              <span role="status">Listening…</span>
              <button className="button secondary" type="button" onClick={cancelSpeech}>
                Cancel dictation
              </button>
            </>
          ) : (
            <button className="button secondary" type="button" onClick={() => void dictate()}>
              Dictate transaction
            </button>
          )}
        </div>
      )}
      <QueryError error={speechError} />
      <Field label="Describe your transaction">
        <textarea
          required
          autoFocus
          maxLength={4000}
          rows={4}
          placeholder="Spent MYR 18.50 on lunch today"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </Field>
      <p className="muted">
        Folio parses this text locally. Review every field before confirming a financial record.
      </p>
    </FinanceDialog>
  );
}
export function Transactions() {
  const { workspace, accounts, categories, refreshFinance } = useFinance();
  const { notify } = useWorkspace();
  const [params] = useSearchParams();
  const [search, setSearch] = useState(''),
    [kind, setKind] = useState(''),
    [accountId, setAccount] = useState(''),
    [categoryId, setCategory] = useState(''),
    [from, setFrom] = useState(''),
    [to, setTo] = useState(''),
    [offset, setOffset] = useState(0);
  const [edit, setEdit] = useState<Transaction | 'new' | null>(params.has('new') ? 'new' : null),
    [remove, setRemove] = useState<Transaction | null>(null),
    [quick, setQuick] = useState(false),
    [draft, setDraft] = useState<EntryDraft | null>(null),
    [suggestion, setSuggestion] = useState<TransactionSuggestion | null>(null),
    [actionError, setActionError] = useState(''),
    [exportPath, setExportPath] = useState('');
  const filter: TransactionFilter = {
    workspaceId: workspace?.id ?? '',
    from: from || null,
    to: to || null,
    accountId: accountId || null,
    categoryId: categoryId || null,
    kind: (kind as TransactionFilter['kind']) || null,
    search: search || null,
    minAmountMinor: null,
    maxAmountMinor: null,
    limit: 50,
    offset,
  };
  const filterKey = JSON.stringify(filter);
  const query = useQuery(
    useCallback(
      () =>
        workspace
          ? api.transactions(JSON.parse(filterKey))
          : Promise.resolve({ items: [], total: 0 }),
      [workspace, filterKey],
    ),
  );
  const suggestions = useQuery(
    useCallback(
      () => (workspace ? api.suggestions(workspace.id) : Promise.resolve([])),
      [workspace],
    ),
  );
  function reset(action: () => void) {
    action();
    setOffset(0);
  }
  async function exportCsv() {
    try {
      const csv = await api.exportTransactionsCsv({ ...filter, offset: 0, limit: 500 });
      setExportPath(csv.path);
      notify(`CSV saved: ${csv.fileName}`);
    } catch (e) {
      setActionError(errorMessage(e));
    }
  }
  function submitSearch(e: FormEvent) {
    e.preventDefault();
    setOffset(0);
  }
  return (
    <FinanceFrame
      title="Transactions"
      subtitle="Income, spending and transfers, separated from corporate claims."
      actions={
        <>
          <button
            className="button secondary"
            disabled={!accounts.length}
            onClick={() => setQuick(true)}
          >
            Quick entry
          </button>
          <button
            className="button primary"
            disabled={!accounts.length}
            onClick={() => setEdit('new')}
          >
            New transaction
          </button>
        </>
      }
    >
      {!accounts.length ? (
        <MissingAccounts />
      ) : (
        <>
          <form className="finance-filters" onSubmit={submitSearch}>
            <Field label="Search transactions">
              <input
                value={search}
                placeholder="Payee or notes"
                onChange={(e) => reset(() => setSearch(e.target.value))}
              />
            </Field>
            <Field label="Kind">
              <select value={kind} onChange={(e) => reset(() => setKind(e.target.value))}>
                <option value="">All kinds</option>
                <option value="income">Income</option>
                <option value="expense">Expense</option>
                <option value="transfer">Transfer</option>
              </select>
            </Field>
            <Field label="Account filter">
              <select value={accountId} onChange={(e) => reset(() => setAccount(e.target.value))}>
                <option value="">All accounts</option>
                <AccountOptions accounts={accounts} includeArchived />
              </select>
            </Field>
            <Field label="Category filter">
              <select value={categoryId} onChange={(e) => reset(() => setCategory(e.target.value))}>
                <option value="">All categories</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="From date">
              <input
                type="date"
                value={from}
                onChange={(e) => reset(() => setFrom(e.target.value))}
              />
            </Field>
            <Field label="To date">
              <input type="date" value={to} onChange={(e) => reset(() => setTo(e.target.value))} />
            </Field>
          </form>
          <QueryError error={query.error || actionError} />
          <Panel
            title="Ledger activity"
            action={
              <button className="text-button" onClick={() => void exportCsv()}>
                Export transactions CSV
              </button>
            }
          >
            {query.loading ? (
              <Loading />
            ) : (
              <TransactionRows items={query.data?.items ?? []} edit={setEdit} />
            )}
            <div className="finance-pagination">
              <button
                className="button secondary"
                disabled={!offset}
                onClick={() => setOffset(Math.max(0, offset - 50))}
              >
                Previous
              </button>
              <span>{query.data?.total ?? 0} matching transactions</span>
              <button
                className="button secondary"
                disabled={offset + 50 >= (query.data?.total ?? 0)}
                onClick={() => setOffset(offset + 50)}
              >
                Next
              </button>
            </div>
            {exportPath && <p className="finance-panel-copy">Saved to {exportPath}</p>}
          </Panel>
          {(suggestions.data?.length ?? 0) > 0 && (
            <Panel title="Payment suggestions">
              <QueryError error={suggestions.error} />
              {suggestions.data!.map((s) => (
                <div className="finance-list-row" key={s.id}>
                  <div>
                    <strong>{s.payee || s.appLabel}</strong>
                    <small>
                      {s.occurredOn} · {formatMoney(s.amountMinor, s.currency)}
                    </small>
                  </div>
                  <button className="button secondary" onClick={() => setSuggestion(s)}>
                    Review suggestion
                  </button>
                  <button
                    className="text-button"
                    onClick={() =>
                      void api
                        .dismissSuggestion(s.id)
                        .then(refreshFinance)
                        .catch((e) => setActionError(errorMessage(e)))
                    }
                  >
                    Dismiss
                  </button>
                </div>
              ))}
            </Panel>
          )}
        </>
      )}
      {(edit || draft || suggestion) && accounts.length > 0 && (
        <>
          <TransactionEditor
            key={edit === 'new' ? 'new' : (edit?.id ?? draft?.text ?? suggestion?.id)}
            transaction={edit && edit !== 'new' ? edit : undefined}
            draft={draft ?? undefined}
            suggestion={suggestion ?? undefined}
            remove={
              edit && edit !== 'new'
                ? () => {
                    setRemove(edit);
                    setEdit(null);
                  }
                : undefined
            }
            close={() => {
              setEdit(null);
              setDraft(null);
              setSuggestion(null);
            }}
          />
        </>
      )}
      {quick && <QuickEntry close={() => setQuick(false)} review={setDraft} />}
      {remove && (
        <ConfirmDelete
          title="Delete transaction?"
          description={
            remove.expenseId
              ? 'The linked receipt is kept. Account balances, budgets and reports will be recalculated.'
              : 'Account balances, budgets and reports will be recalculated. A linked bill occurrence becomes unpaid.'
          }
          close={() => setRemove(null)}
          remove={async () => {
            await api.deleteTransactions([remove.id]);
            await refreshFinance();
            notify('Transaction deleted.');
          }}
        />
      )}
    </FinanceFrame>
  );
}
