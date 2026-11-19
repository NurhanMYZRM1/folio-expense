import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from 'react';
import { Link, NavLink } from 'react-router-dom';
import type { Account, Category, Workspace } from '../../bindings/generated';
import { api, desktop, localToday } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { formatMoney, moneyInput, parseMoney } from '../../lib/money';
import { EmptyState, Loading, PageHeader } from '../../components/ui';
import { Modal } from '../../components/motion';

type FinanceStore = {
  workspaces: Workspace[];
  workspace: Workspace | null;
  accounts: Account[];
  categories: Category[];
  loading: boolean;
  error: string;
  revision: number;
  selectWorkspace: (id: string) => void;
  refreshFinance: () => Promise<void>;
};
const FinanceContext = createContext<FinanceStore | null>(null);
export function FinanceProvider({ children }: { children: ReactNode }) {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selected, setSelected] = useState('');
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [loading, setLoading] = useState(desktop);
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const request = useRef(0);
  const refreshFinance = useCallback(async () => {
    if (!desktop && !('__FOLIO_TEST__' in window)) return;
    const number = ++request.current;
    try {
      const next = await api.workspaces();
      const id =
        next.find((w) => w.id === selected)?.id ??
        next.find((w) => w.kind === 'personal')?.id ??
        next[0]?.id;
      const [a, c] = id ? await Promise.all([api.accounts(id), api.categories(id)]) : [[], []];
      if (number !== request.current) return;
      setWorkspaces(next);
      setSelected(id ?? '');
      setAccounts(a);
      setCategories(c);
      setRevision((v) => v + 1);
      setError('');
    } catch (e) {
      if (number === request.current) setError(errorMessage(e));
    } finally {
      if (number === request.current) setLoading(false);
    }
  }, [selected]);
  useEffect(() => {
    void Promise.resolve().then(refreshFinance);
  }, [refreshFinance]);
  useEffect(() => {
    const sync = () => void refreshFinance();
    window.addEventListener('folio:cloud-synced', sync);
    return () => window.removeEventListener('folio:cloud-synced', sync);
  }, [refreshFinance]);
  return (
    <FinanceContext.Provider
      value={{
        workspaces,
        workspace: workspaces.find((w) => w.id === selected) ?? null,
        accounts,
        categories,
        loading,
        error,
        revision,
        selectWorkspace: setSelected,
        refreshFinance,
      }}
    >
      {children}
    </FinanceContext.Provider>
  );
}
export function useFinance() {
  const store = useContext(FinanceContext);
  if (!store) throw new Error('Finance provider missing');
  return store;
}
export function useQuery<T>(loader: () => Promise<T>) {
  const revision = useContext(FinanceContext)?.revision ?? 0;
  const [state, setState] = useState<{
    loader: typeof loader;
    revision: number;
    data: T | null;
    error: string;
  } | null>(null);
  useEffect(() => {
    let active = true;
    loader()
      .then((data) => {
        if (active) setState({ loader, revision, data, error: '' });
      })
      .catch((e) => {
        if (active) setState({ loader, revision, data: null, error: errorMessage(e) });
      });
    return () => {
      active = false;
    };
  }, [loader, revision]);
  const current = state?.loader === loader && state.revision === revision;
  return {
    data: current ? state.data : null,
    error: current ? state.error : '',
    loading: !current,
  };
}
export function QueryError({ error }: { error: string }) {
  return error ? (
    <div className="inline-error" role="alert">
      {error}
    </div>
  ) : null;
}
export const tabs = [
  ['/finance', 'Overview'],
  ['/finance/accounts', 'Accounts & goals'],
  ['/finance/transactions', 'Transactions'],
  ['/finance/budgets', 'Budgets'],
  ['/finance/bills', 'Bills & subscriptions'],
  ['/finance/reports', 'Reports'],
  ['/finance/business', 'Business'],
  ['/finance/tax', 'Tax helpers'],
] as const;
export function FinanceFrame({
  title,
  subtitle,
  actions,
  children,
}: {
  title: string;
  subtitle: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const { workspace, workspaces, loading, error, selectWorkspace, refreshFinance } = useFinance();
  return (
    <>
      <PageHeader
        eyebrow="PERSONAL & BUSINESS FINANCE"
        title={title}
        subtitle={subtitle}
        actions={actions}
      />
      <div className="finance-context">
        <label className="field">
          <span>Finance workspace</span>
          <select
            aria-label="Finance workspace"
            value={workspace?.id ?? ''}
            onChange={(e) => selectWorkspace(e.target.value)}
          >
            {!workspaces.length && <option value="">No workspace available</option>}
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>
                {w.name} · {w.kind} · {w.baseCurrency}
              </option>
            ))}
          </select>
        </label>
        <span className="muted">
          Saved on this device · {workspace?.baseCurrency ?? 'local currency'}
        </span>
      </div>
      <nav className="finance-tabs" aria-label="Finance">
        {tabs.map(([to, label]) => (
          <NavLink key={to} to={to} end={to === '/finance'}>
            {label}
          </NavLink>
        ))}
      </nav>
      {error ? (
        <div className="inline-error" role="alert">
          {error}
          <button className="text-button" onClick={() => void refreshFinance()}>
            Retry finance
          </button>
        </div>
      ) : loading ? (
        <Loading />
      ) : workspace ? (
        <div key={workspace.id}>{children}</div>
      ) : (
        <EmptyState
          title="Your finances start here"
          description="Open Folio with its local core to create your personal finance workspace."
        />
      )}
    </>
  );
}
export function FinanceDialog({
  title,
  close,
  save,
  children,
  label = 'Save',
}: {
  title: string;
  close: () => void;
  save: () => Promise<void>;
  children: ReactNode;
  label?: string;
}) {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError('');
    try {
      await save();
      close();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal open onClose={() => !busy && close()} labelledBy="finance-dialog-title">
      <h2 id="finance-dialog-title">{title}</h2>
      <QueryError error={error} />
      <form className="finance-dialog-form" onSubmit={(e) => void submit(e)}>
        <fieldset disabled={busy}>
          {children}
          <div className="modal-actions">
            <button type="button" className="button secondary" onClick={close}>
              Cancel
            </button>
            <button className="button primary" type="submit">
              {busy ? 'Saving…' : label}
            </button>
          </div>
        </fieldset>
      </form>
    </Modal>
  );
}
export function ConfirmDelete({
  title,
  description,
  close,
  remove,
}: {
  title: string;
  description: string;
  close: () => void;
  remove: () => Promise<void>;
}) {
  return (
    <FinanceDialog title={title} close={close} save={remove} label="Delete">
      <p>{description}</p>
    </FinanceDialog>
  );
}
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function signedMoney(value: number, currency: string) {
  return `${value < 0 ? '−' : ''}${formatMoney(Math.abs(value), currency)}`;
}
export function signedInput(value: number, currency: string) {
  return `${value < 0 ? '-' : ''}${moneyInput(Math.abs(value), currency)}`;
}
export function parseSigned(value: string, currency: string) {
  return (
    (value.trim().startsWith('-') ? -1 : 1) *
    (parseMoney(value.trim().replace(/^-/, ''), currency) ?? 0)
  );
}
export function requiredMoney(value: string, currency: string) {
  const n = parseMoney(value, currency);
  if (!n) throw new Error('Enter an amount greater than zero.');
  return n;
}
export function MonthPicker({ value, set }: { value: string; set: (v: string) => void }) {
  return (
    <Field label="Calendar month">
      <input
        type="month"
        value={value}
        required
        onChange={(e) => set(e.target.value || localToday().slice(0, 7))}
      />
    </Field>
  );
}
export function MissingAccounts() {
  return (
    <EmptyState
      title="Add an account first"
      description="Create a cash, bank or e-wallet account to record your finances."
      action={
        <Link className="button primary" to="/finance/accounts">
          Manage accounts
        </Link>
      }
    />
  );
}
export function AccountOptions({
  accounts,
  includeArchived = false,
}: {
  accounts: Account[];
  includeArchived?: boolean;
}) {
  return (
    <>
      {accounts
        .filter((a) => includeArchived || !a.archivedAt)
        .map((a) => (
          <option key={a.id} value={a.id}>
            {a.name} · {a.currency}
            {a.archivedAt ? ' · archived' : ''}
          </option>
        ))}
    </>
  );
}
