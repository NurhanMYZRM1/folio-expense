import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import type { Claim, Expense, Job, Settings } from '../bindings/generated';
import { api, desktop } from '../lib/ipc';
import { errorMessage } from '../lib/errors';
import { X, CheckCircle2, AlertCircle } from 'lucide-react';
import { startJobRunner } from '../lib/jobs';
export const defaultSettings: Settings = {
  theme: 'system',
  defaultCurrency: 'MYR',
  onlineEnabled: false,
  provider: 'openai_compatible',
  apiBaseUrl: 'https://api.openai.com/v1',
  aiModel: 'gpt-4o-mini',
  offlineOcrEnabled: true,
  exportDirectory: null,
  includeReceipts: true,
  company: '',
  employee: '',
  currencyConversionEnabled: true,
};
type Notice = { id: number; text: string; error: boolean };
type Store = {
  expenses: Expense[];
  claims: Claim[];
  jobs: Job[];
  settings: Settings;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  notify: (message: string, error?: boolean) => void;
  setSettings: (s: Settings) => void;
  processing: string | null;
};
const Context = createContext<Store | null>(null);
export function Providers({ children }: { children: ReactNode }) {
  const [expenses, setExpenses] = useState<Expense[]>([]),
    [claims, setClaims] = useState<Claim[]>([]),
    [jobs, setJobs] = useState<Job[]>([]);
  const [settings, setSettings] = useState<Settings>(defaultSettings),
    [loading, setLoading] = useState(desktop),
    [error, setError] = useState<string | null>(null);
  const [notices, setNotices] = useState<Notice[]>([]),
    [processing, setProcessing] = useState<string | null>(null);
  const request = useRef(0);
  const notify = useCallback((text: string, error = false) => {
    const id = Date.now() + Math.random();
    setNotices((n) => [...n.slice(-3), { id, text, error }]);
    if (!error) window.setTimeout(() => setNotices((n) => n.filter((v) => v.id !== id)), 7000);
  }, []);
  const refresh = useCallback(async () => {
    if (!desktop && !('__FOLIO_TEST__' in window)) {
      setLoading(false);
      return;
    }
    const number = ++request.current;
    try {
      const [e, c, j, s] = await Promise.all([
        api.expenses(),
        api.claims(),
        api.jobs(),
        api.settings(),
      ]);
      if (number !== request.current) return;
      setExpenses(e);
      setClaims(c);
      setJobs(j);
      setSettings(s);
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
    if (!desktop) return;
    return startJobRunner({ refresh, notify, progress: setProcessing });
  }, [refresh, notify]);
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () =>
      (document.documentElement.dataset.theme =
        settings.theme === 'system' ? (media.matches ? 'dark' : 'light') : settings.theme);
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [settings.theme]);
  return (
    <Context.Provider
      value={{
        expenses,
        claims,
        jobs,
        settings,
        loading,
        error,
        refresh,
        notify,
        setSettings,
        processing,
      }}
    >
      {children}
      <div className="toasts" aria-live="polite">
        {notices.map((n) => (
          <div className={`toast ${n.error ? 'error' : ''}`} key={n.id}>
            {n.error ? <AlertCircle size={18} /> : <CheckCircle2 size={18} />}
            <span>{n.text}</span>
            <button
              className="icon-button"
              aria-label="Dismiss notification"
              onClick={() => setNotices((v) => v.filter((x) => x.id !== n.id))}
            >
              <X size={16} />
            </button>
          </div>
        ))}
      </div>
    </Context.Provider>
  );
}
export function useWorkspace(): Store {
  const context = useContext(Context);
  if (!context) throw new Error('Workspace provider missing');
  return context;
}
