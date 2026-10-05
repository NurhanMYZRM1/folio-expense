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
import { X, AlertCircle } from 'lucide-react';
import { startJobRunner } from '../lib/jobs';
import { AnimatePresence, motion, useIsPresent } from 'motion/react';
import { SuccessCheck, spring } from '../components/motion';
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
        <AnimatePresence initial={false} mode="popLayout">
          {notices.map((n) => (
            <Toast
              key={n.id}
              error={n.error}
              text={n.text}
              onDismiss={() => setNotices((v) => v.filter((x) => x.id !== n.id))}
            />
          ))}
        </AnimatePresence>
      </div>
    </Context.Provider>
  );
}
export function useWorkspace(): Store {
  const context = useContext(Context);
  if (!context) throw new Error('Workspace provider missing');
  return context;
}

/**
 * A toast leaves the accessibility tree and stops taking pointer input the
 * moment it is dismissed, so its exit animation can never be clicked again or
 * announced twice.
 */
function Toast({
  error,
  text,
  onDismiss,
}: {
  error?: boolean;
  text: string;
  onDismiss: () => void;
}) {
  const present = useIsPresent();
  return (
    <motion.div
      layout={present ? 'position' : false}
      className={`toast ${error ? 'error' : ''}`}
      aria-hidden={present ? undefined : true}
      inert={!present}
      style={{ pointerEvents: present ? undefined : 'none' }}
      initial={{ opacity: 0, y: 24, scale: 0.92 }}
      animate={{ opacity: 1, y: 0, scale: 1, transition: spring.bouncy }}
      exit={{
        opacity: 0,
        x: 40,
        scale: 0.96,
        transition: { duration: 0.18, ease: [0.4, 0, 1, 1] },
      }}
      transition={spring.smooth}
    >
      <span className="toast-icon">
        {error ? <AlertCircle size={18} /> : <SuccessCheck size={20} />}
      </span>
      <span>{text}</span>
      <button className="icon-button" aria-label="Dismiss notification" onClick={onDismiss}>
        <X size={16} />
      </button>
    </motion.div>
  );
}
