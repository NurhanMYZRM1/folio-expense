import { runSingleFlight, startForegroundSync } from '../lib/cloudSync';
import { FinanceProvider } from '../features/finance/shared';
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
import { api, desktop, type CloudOutcome, type CloudProvider, type CloudStatus } from '../lib/ipc';
import { errorMessage } from '../lib/errors';
import { X, AlertCircle } from 'lucide-react';
import { startJobRunner } from '../lib/jobs';
import { AnimatePresence, motion, useIsPresent } from 'motion/react';
import { SuccessCheck, spring, trackVisibleArea } from '../components/motion';
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
  notifyBills: true,
  notifyBudgets: true,
  paymentAlertsEnabled: false,
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
  cloud: CloudStatus | null;
  cloudBusy: boolean;
  cloudError: string;
  /** `force` is a person asking; it is not held back by retry backoff. */
  syncCloud: (statusOnly?: boolean, force?: boolean) => Promise<void>;
  signInCloud: (email: string, secret: string, method?: 'password' | 'code') => Promise<void>;
  /** Google or Apple in the system browser; `link` adds it to the signed-in account. */
  signInWithProvider: (provider: CloudProvider, link?: boolean) => Promise<void>;
  /** The desktop's browser came back with this address. */
  finishProviderSignIn: (url: string) => Promise<void>;
  cancelProviderSignIn: () => void;
  /** True while the desktop waits for the browser to return. */
  cloudBrowser: boolean;
  /** Set when records already on this device need a decision before joining an account. */
  cloudChoice: { email: string | null } | null;
  adoptCloudData: (choice: 'join' | 'separate' | 'cancel') => Promise<void>;
  signOutCloud: () => Promise<void>;
  resolveCloudConflict: (id: string, choice: 'local' | 'remote') => Promise<void>;
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
  const [cloud, setCloud] = useState<CloudStatus | null>(null);
  const [cloudBusy, setCloudBusy] = useState(false);
  const [cloudError, setCloudError] = useState('');
  const [cloudBrowser, setCloudBrowser] = useState(false);
  const [cloudChoice, setCloudChoice] = useState<{ email: string | null } | null>(null);
  const cloudTask = useRef<Promise<void> | null>(null);
  const cloudGeneration = useRef(0);
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
      if (number === request.current) setError(errorMessage(e));
    } finally {
      // An older refresh must not mount forms with defaults while the current
      // request is still loading their saved values.
      if (number === request.current) setLoading(false);
    }
  }, []);
  useEffect(() => trackVisibleArea(), []);
  useEffect(() => {
    void refresh();
    if (!desktop) return;
    return startJobRunner({ refresh, notify, progress: setProcessing });
  }, [refresh, notify]);
  // All calls share one promise, including manual actions and foreground retries.
  // A generation change prevents a completed old request from restoring signed-out UI.
  const syncCloud = useCallback(
    (statusOnly = false, force = false, onlyIfPending = false) =>
      runSingleFlight(cloudTask, async () => {
        const generation = cloudGeneration.current;
        try {
          const status = await api.cloudStatus();
          if (generation !== cloudGeneration.current) return;
          setCloud(status);
          if (!status.signedIn || statusOnly || (onlyIfPending && !status.pending)) return;
          setCloudBusy(true);
          const result = await api.cloudSync(force);
          if (generation !== cloudGeneration.current) return;
          setCloud(result);
          setCloudError('');
          await refresh();
          window.dispatchEvent(new Event('folio:cloud-synced'));
        } catch (error) {
          if (generation === cloudGeneration.current) setCloudError(errorMessage(error));
        } finally {
          if (generation === cloudGeneration.current) setCloudBusy(false);
        }
      }),
    [refresh],
  );
  // Every way of signing in ends here. Changing account reloads the window so
  // nothing the previous account loaded stays on screen or in memory.
  const applyOutcome = useCallback(
    async (result: CloudOutcome | null) => {
      if (!result) return;
      setCloudBrowser(false);
      if (result.outcome === 'chooseData') {
        setCloudChoice({ email: result.email });
        return;
      }
      setCloudChoice(null);
      setCloud(result.status);
      if (result.outcome === 'switched') {
        if ('__folio' in window) {
          await refresh();
          window.dispatchEvent(new Event('folio:cloud-synced'));
        } else window.location.reload();
        return;
      }
      if (result.outcome === 'cancelled') return;
      // Finish any stale poll before starting the newly authenticated session.
      await cloudTask.current;
      await syncCloud('__folio' in window, true);
    },
    [refresh, syncCloud],
  );
  const authenticate = useCallback(
    async (attempt: () => Promise<CloudOutcome | null>) => {
      const generation = ++cloudGeneration.current;
      setCloudBusy(true);
      setCloudError('');
      let result: CloudOutcome | null;
      try {
        result = await attempt();
      } catch (error) {
        if (generation === cloudGeneration.current) setCloudError(errorMessage(error));
        throw error;
      } finally {
        if (generation === cloudGeneration.current) setCloudBusy(false);
      }
      if (generation === cloudGeneration.current) await applyOutcome(result);
    },
    [applyOutcome],
  );
  const signInCloud = useCallback(
    (email: string, secret: string, method: 'password' | 'code' = 'password') =>
      authenticate(() =>
        method === 'code' ? api.cloudVerifyCode(email, secret) : api.cloudSignIn(email, secret),
      ),
    [authenticate],
  );
  const signInWithProvider = useCallback(
    (provider: CloudProvider, link = false) =>
      authenticate(async () => {
        const result = await api.cloudOauthStart(provider, link);
        // The desktop continues in the browser and returns through a deep link.
        if (!result || !('outcome' in result)) setCloudBrowser(true);
        return result && 'outcome' in result ? result : null;
      }),
    [authenticate],
  );
  const finishProviderSignIn = useCallback(
    (url: string) => authenticate(() => api.cloudOauthFinish(url)).catch(() => undefined),
    [authenticate],
  );
  const cancelProviderSignIn = useCallback(() => {
    setCloudBrowser(false);
    void api.cloudOauthCancel().catch(() => undefined);
  }, []);
  const adoptCloudData = useCallback(
    (choice: 'join' | 'separate' | 'cancel') => authenticate(() => api.cloudAdopt(choice)),
    [authenticate],
  );
  const signOutCloud = useCallback(async () => {
    const generation = ++cloudGeneration.current;
    setCloudBusy(true);
    try {
      const status = await api.cloudSignOut();
      if (generation !== cloudGeneration.current) return;
      setCloud(status);
      setCloudError('');
    } catch (error) {
      if (generation === cloudGeneration.current) setCloudError(errorMessage(error));
      throw error;
    } finally {
      if (generation === cloudGeneration.current) setCloudBusy(false);
    }
  }, []);
  const resolveCloudConflict = useCallback(
    async (id: string, choice: 'local' | 'remote') => {
      // Let the current pull finish so its result cannot overwrite a resolution.
      await cloudTask.current;
      const generation = cloudGeneration.current;
      setCloudBusy(true);
      try {
        const status = await api.cloudResolveConflict(id, choice);
        if (generation !== cloudGeneration.current) return;
        setCloud(status);
        setCloudError('');
        await refresh();
        window.dispatchEvent(new Event('folio:cloud-synced'));
      } catch (error) {
        if (generation === cloudGeneration.current) setCloudError(errorMessage(error));
        throw error;
      } finally {
        if (generation === cloudGeneration.current) setCloudBusy(false);
      }
    },
    [refresh],
  );
  useEffect(() => {
    if (!desktop) return;
    // Phones run one native sync loop (mobile/src/core/folio.ts); each embedded screen only refreshes status.
    return startForegroundSync(() => syncCloud('__folio' in window));
  }, [syncCloud]);
  useEffect(() => {
    if (!desktop) return;
    // Upload shortly after an edit instead of waiting for the next poll.
    let timer: number | undefined;
    const changed = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void syncCloud('__folio' in window, false, true), 2000);
    };
    window.addEventListener('folio:changed', changed);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener('folio:changed', changed);
    };
  }, [syncCloud]);
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
        cloud,
        cloudBusy,
        cloudError,
        syncCloud,
        signInCloud,
        signInWithProvider,
        finishProviderSignIn,
        cancelProviderSignIn,
        cloudBrowser,
        cloudChoice,
        adoptCloudData,
        signOutCloud,
        resolveCloudConflict,
      }}
    >
      <FinanceProvider>{children}</FinanceProvider>
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
