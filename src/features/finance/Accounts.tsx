import { useState } from 'react';
import type { Account, AccountInput, Category, Workspace } from '../../bindings/generated';
import { api } from '../../lib/ipc';
import { CURRENCIES, moneyInput } from '../../lib/money';
import { EmptyState, Panel } from '../../components/ui';
import { useWorkspace } from '../../app/providers';
import {
  ConfirmDelete,
  Field,
  FinanceDialog,
  FinanceFrame,
  parseSigned,
  requiredMoney,
  signedInput,
  signedMoney,
  useFinance,
} from './shared';

function AccountEditor({ account, close }: { account?: Account; close: () => void }) {
  const { workspace, refreshFinance } = useFinance();
  const [form, setForm] = useState({
    name: account?.name ?? '',
    kind: account?.kind ?? 'bank',
    currency: account?.currency ?? workspace!.baseCurrency,
    opening: signedInput(
      account?.openingBalanceMinor ?? 0,
      account?.currency ?? workspace!.baseCurrency,
    ),
    goal: account?.goalTargetMinor ? moneyInput(account.goalTargetMinor, account.currency) : '',
    targetDate: account?.goalTargetDate ?? '',
    direction: account?.loanDirection ?? 'borrowed',
    principal: account?.loanPrincipalMinor
      ? moneyInput(account.loanPrincipalMinor, account.currency)
      : '',
    rate: account?.loanAnnualRate ?? '',
    counterparty: account?.counterparty ?? '',
    note: account?.note ?? '',
    archived: !!account?.archivedAt,
  });
  const update = <K extends keyof typeof form>(key: K, value: (typeof form)[K]) =>
    setForm((v) => ({ ...v, [key]: value }));
  return (
    <FinanceDialog
      title={account ? 'Edit account' : 'New account'}
      close={close}
      save={async () => {
        const input: AccountInput = {
          id: account?.id ?? null,
          version: account?.version ?? null,
          workspaceId: workspace!.id,
          name: form.name.trim(),
          kind: form.kind,
          currency: form.currency,
          openingBalanceMinor: parseSigned(form.opening, form.currency),
          goalTargetMinor:
            form.kind === 'savings' && form.goal ? requiredMoney(form.goal, form.currency) : null,
          goalTargetDate: form.kind === 'savings' ? form.targetDate || null : null,
          loanDirection: form.kind === 'loan' ? form.direction : null,
          loanPrincipalMinor:
            form.kind === 'loan' ? requiredMoney(form.principal, form.currency) : null,
          loanAnnualRate: form.kind === 'loan' ? form.rate || null : null,
          counterparty: form.counterparty || null,
          note: form.note,
          archived: form.archived,
        };
        await api.saveAccount(input);
        await refreshFinance();
      }}
    >
      <Field label="Account name">
        <input
          autoFocus
          required
          maxLength={120}
          value={form.name}
          onChange={(e) => update('name', e.target.value)}
        />
      </Field>
      <div className="form-grid">
        <Field label="Account type">
          <select
            value={form.kind}
            onChange={(e) => update('kind', e.target.value as Account['kind'])}
          >
            {['bank', 'cash', 'ewallet', 'card', 'savings', 'loan', 'other'].map((k) => (
              <option key={k} value={k}>
                {k === 'savings' ? 'Savings goal' : k === 'ewallet' ? 'E-wallet' : k}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Account currency">
          <select value={form.currency} onChange={(e) => update('currency', e.target.value)}>
            {CURRENCIES.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
        </Field>
      </div>
      <Field label="Opening balance">
        <input
          inputMode="decimal"
          required
          value={form.opening}
          onChange={(e) => update('opening', e.target.value)}
        />
      </Field>
      <p className="muted">
        Use a negative opening balance for money owed, including a borrowed loan. Opening balances
        are excluded from income and spending.
      </p>
      {form.kind === 'savings' && (
        <div className="form-grid">
          <Field label="Savings target">
            <input
              inputMode="decimal"
              value={form.goal}
              onChange={(e) => update('goal', e.target.value)}
            />
          </Field>
          <Field label="Target date">
            <input
              type="date"
              value={form.targetDate}
              onChange={(e) => update('targetDate', e.target.value)}
            />
          </Field>
        </div>
      )}
      {form.kind === 'loan' && (
        <>
          <div className="form-grid">
            <Field label="Loan direction">
              <select
                value={form.direction}
                onChange={(e) => update('direction', e.target.value as 'borrowed' | 'lent')}
              >
                <option value="borrowed">I borrowed</option>
                <option value="lent">I lent</option>
              </select>
            </Field>
            <Field label="Original principal">
              <input
                required
                inputMode="decimal"
                value={form.principal}
                onChange={(e) => update('principal', e.target.value)}
              />
            </Field>
          </div>
          <Field label="Annual rate (informational only)">
            <input value={form.rate} onChange={(e) => update('rate', e.target.value)} />
          </Field>
          <p className="muted">
            No interest or repayment projection is made. Record repayments as transfers and interest
            as an expense.
          </p>
        </>
      )}
      <Field label="Counterparty">
        <input
          maxLength={120}
          value={form.counterparty}
          onChange={(e) => update('counterparty', e.target.value)}
        />
      </Field>
      <Field label="Account notes">
        <textarea
          rows={2}
          maxLength={2000}
          value={form.note}
          onChange={(e) => update('note', e.target.value)}
        />
      </Field>
      {account && (
        <label className="check-field">
          <input
            type="checkbox"
            checked={form.archived}
            onChange={(e) => update('archived', e.target.checked)}
          />{' '}
          Archive account (requires a zero balance)
        </label>
      )}
    </FinanceDialog>
  );
}
function WorkspaceEditor({ workspace, close }: { workspace?: Workspace; close: () => void }) {
  const { refreshFinance } = useFinance();
  const [name, setName] = useState(workspace?.name ?? ''),
    [currency, setCurrency] = useState(workspace?.baseCurrency ?? 'MYR');
  return (
    <FinanceDialog
      title={workspace ? 'Edit finance workspace' : 'New business workspace'}
      close={close}
      save={async () => {
        await api.saveWorkspace({
          id: workspace?.id ?? null,
          version: workspace?.version ?? null,
          name: name.trim(),
          baseCurrency: currency,
        });
        await refreshFinance();
      }}
    >
      <Field label="Workspace name">
        <input
          required
          autoFocus
          maxLength={120}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      <Field label="Reporting currency">
        <select value={currency} onChange={(e) => setCurrency(e.target.value)}>
          {CURRENCIES.map((c) => (
            <option key={c}>{c}</option>
          ))}
        </select>
      </Field>
      <p className="muted">
        {workspace
          ? 'Changing the reporting currency recalculates transaction conversions. Transactions without an available rate stay excluded from reports.'
          : 'Business records stay separate from personal finances. This creates a local workspace.'}
      </p>
    </FinanceDialog>
  );
}
function CategoryEditor({ category, close }: { category?: Category; close: () => void }) {
  const { workspace, refreshFinance } = useFinance();
  const [name, setName] = useState(category?.name ?? ''),
    [kind, setKind] = useState(category?.kind ?? 'expense'),
    [archived, setArchived] = useState(!!category?.archivedAt);
  return (
    <FinanceDialog
      title={category ? 'Edit category' : 'New category'}
      close={close}
      save={async () => {
        await api.saveCategory({
          id: category?.id ?? null,
          version: category?.version ?? null,
          workspaceId: workspace!.id,
          name: name.trim(),
          kind,
          archived,
        });
        await refreshFinance();
      }}
    >
      <Field label="Category name">
        <input required maxLength={60} value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label="Category kind">
        <select value={kind} onChange={(e) => setKind(e.target.value as 'income' | 'expense')}>
          <option value="expense">Expense</option>
          <option value="income">Income</option>
        </select>
      </Field>
      {category && (
        <label className="check-field">
          <input
            type="checkbox"
            checked={archived}
            onChange={(e) => setArchived(e.target.checked)}
          />{' '}
          Archive category
        </label>
      )}
    </FinanceDialog>
  );
}
export function Accounts() {
  const { workspace, accounts, categories, refreshFinance } = useFinance();
  const { notify } = useWorkspace();
  const [edit, setEdit] = useState<Account | 'new' | null>(null),
    [remove, setRemove] = useState<Account | null>(null),
    [workspaceEdit, setWorkspaceEdit] = useState<'edit' | 'new' | null>(null),
    [categoryEdit, setCategoryEdit] = useState<Category | 'new' | null>(null);
  return (
    <FinanceFrame
      title="Accounts & goals"
      subtitle="Balances, savings goals and loans, with every movement accounted for."
      actions={
        <button className="button primary" onClick={() => setEdit('new')}>
          New account
        </button>
      }
    >
      <div className="finance-card-grid">
        {accounts.map((a) => (
          <section className="panel finance-account" key={a.id}>
            <div className="panel-heading">
              <h2>{a.name}</h2>
              <span className="badge">{a.archivedAt ? 'Archived' : a.kind}</span>
            </div>
            <strong className="finance-balance">{signedMoney(a.balanceMinor, a.currency)}</strong>
            {a.goalTargetMinor && (
              <>
                <p>
                  Target {signedMoney(a.goalTargetMinor, a.currency)}
                  {a.goalTargetDate && ` by ${a.goalTargetDate}`}
                </p>
                <progress
                  aria-label={`${a.name} savings progress`}
                  value={Math.max(0, a.balanceMinor)}
                  max={a.goalTargetMinor}
                />
              </>
            )}
            {a.kind === 'loan' && (
              <>
                <p>
                  {a.loanDirection === 'borrowed' ? 'You owe' : 'Owed to you'}:{' '}
                  {signedMoney(Math.abs(a.balanceMinor), a.currency)}
                </p>
                <p>
                  Repaid{' '}
                  {signedMoney(
                    Math.max(0, (a.loanPrincipalMinor ?? 0) - Math.abs(a.balanceMinor)),
                    a.currency,
                  )}{' '}
                  of {signedMoney(a.loanPrincipalMinor ?? 0, a.currency)}
                </p>
              </>
            )}
            {a.note && <p className="muted">{a.note}</p>}
            <div className="finance-row-actions">
              <button className="text-button" onClick={() => setEdit(a)}>
                Edit {a.name}
              </button>
              <button className="text-button danger-text" onClick={() => setRemove(a)}>
                Delete {a.name}
              </button>
            </div>
          </section>
        ))}
      </div>
      {!accounts.length && (
        <EmptyState
          title="Where do you keep your money?"
          description="Add your bank account, cash or e-wallet. Savings goals and loans use the same ledger."
        />
      )}
      <Panel
        title="Categories"
        action={
          <button className="text-button" onClick={() => setCategoryEdit('new')}>
            New category
          </button>
        }
      >
        <div className="finance-category-list">
          {categories.map((c) => (
            <button className="button secondary" key={c.id} onClick={() => setCategoryEdit(c)}>
              {c.name} · {c.kind}
              {c.archivedAt ? ' · archived' : ''}
            </button>
          ))}
        </div>
      </Panel>
      <Panel title="Workspace settings">
        <p className="finance-panel-copy">
          Personal and business accounts are kept separate. Corporate receipts and claims remain in
          the receipt workspace.
        </p>
        <div className="finance-row-actions">
          <button className="button secondary" onClick={() => setWorkspaceEdit('edit')}>
            Edit workspace
          </button>
          <button className="button secondary" onClick={() => setWorkspaceEdit('new')}>
            New business workspace
          </button>
        </div>
      </Panel>
      {edit && (
        <AccountEditor
          key={typeof edit === 'string' ? 'new' : edit.id}
          account={edit === 'new' ? undefined : edit}
          close={() => setEdit(null)}
        />
      )}
      {remove && (
        <ConfirmDelete
          title={`Delete ${remove.name}?`}
          description="Only an unused account can be deleted. Archive a zero-balance account to keep its transaction history."
          close={() => setRemove(null)}
          remove={async () => {
            await api.deleteAccount(remove.id);
            await refreshFinance();
            notify('Account deleted.');
          }}
        />
      )}
      {workspaceEdit && (
        <WorkspaceEditor
          workspace={workspaceEdit === 'edit' ? workspace! : undefined}
          close={() => setWorkspaceEdit(null)}
        />
      )}
      {categoryEdit && (
        <CategoryEditor
          category={categoryEdit === 'new' ? undefined : categoryEdit}
          close={() => setCategoryEdit(null)}
        />
      )}
    </FinanceFrame>
  );
}
