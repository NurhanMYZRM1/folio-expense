import { useEffect } from 'react';
import {
  HashRouter,
  NavLink,
  Route,
  Routes,
  useLocation,
  useNavigate,
  Link,
} from 'react-router-dom';
import {
  LayoutDashboard,
  Rows3,
  Files,
  Upload,
  Settings2,
  ShieldCheck,
  ArrowUpRight,
  ChevronRight,
  Sun,
  Moon,
  LoaderCircle,
} from 'lucide-react';
import { getCurrent, onOpenUrl } from '@tauri-apps/plugin-deep-link';
import { Providers, useWorkspace } from './providers';
import { api, desktop } from '../lib/ipc';
import { errorMessage } from '../lib/errors';
import { Dashboard } from '../features/dashboard/Dashboard';
import { Expenses } from '../features/expenses/Expenses';
import { ExpenseDetail } from '../features/expenses/ExpenseDetail';
import { ImportReceipts } from '../features/receipts/ImportReceipts';
import { Claims, ClaimDetail } from '../features/claims/Claims';
import { Settings } from '../features/settings/Settings';
import { PageSkeleton } from '../components/ui';
import { PageTransition, spring } from '../components/motion';
import { AnimatePresence, LayoutGroup, MotionConfig, motion } from 'motion/react';
function NavPill({ id }: { id: string }) {
  return <motion.span layoutId={id} className="nav-pill" transition={spring.indicator} />;
}
const navigation = [
  { to: '/', label: 'Overview', icon: LayoutDashboard },
  { to: '/expenses', label: 'Expenses', icon: Rows3 },
  { to: '/claims', label: 'Claims', icon: Files },
  { to: '/import', label: 'Import receipts', icon: Upload },
];
function Shell() {
  const { settings, expenses, loading, error, refresh, notify, setSettings, processing } =
    useWorkspace();
  const location = useLocation(),
    navigate = useNavigate();
  const pending = expenses.filter((e) => e.status === 'needs_review').length;
  useEffect(() => {
    if (!desktop) return;
    let cleanup: (() => void) | undefined,
      cancelled = false;
    const open = async (urls: string[]) => {
      for (const value of urls) {
        try {
          const url = new URL(value);
          if (url.protocol !== 'expenseapp:' || url.hostname !== 'receipt') continue;
          const id = url.pathname.slice(1);
          const found = (await api.expenses()).find((e) => e.receiptId === id);
          if (found) navigate(`/expenses/${found.id}`);
          else notify('This receipt is not in this local workspace.', true);
        } catch {
          notify('The receipt link could not be opened.', true);
        }
      }
    };
    void getCurrent().then((urls) => {
      if (urls && !cancelled) void open(urls);
    });
    void onOpenUrl((urls) => void open(urls)).then((fn) => {
      if (cancelled) fn();
      else cleanup = fn;
    });
    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, [navigate, notify]);
  async function toggleTheme() {
    const next = {
      ...settings,
      theme: document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark',
    };
    if (!desktop) {
      setSettings(next);
      return;
    }
    try {
      setSettings(await api.saveSettings(next));
    } catch (e) {
      notify(errorMessage(e), true);
    }
  }
  const section =
    navigation.find((n) => n.to !== '/' && location.pathname.startsWith(n.to))?.label ??
    (location.pathname === '/settings' ? 'Settings' : 'Overview');
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <Link to="/" className="brand">
          <span className="brand-mark">
            f<span>•</span>
          </span>
          <span>
            folio<span className="brand-caption">EXPENSE WORKSPACE</span>
          </span>
        </Link>
        <div className="workspace-card">
          <div className="workspace-avatar">{settings.company ? settings.company[0] : 'P'}</div>
          <div>
            <strong>{settings.company || 'Personal workspace'}</strong>
            <span>Local workspace</span>
          </div>
          <ShieldCheck size={15} />
        </div>
        <div className="nav-label">WORKSPACE</div>
        <nav>
          {navigation.map((n) => (
            <NavLink key={n.to} to={n.to} end={n.to === '/'}>
              {({ isActive }) => (
                <>
                  {isActive && <NavPill id="sidebar-pill" />}
                  <n.icon size={18} strokeWidth={1.7} />
                  <span>{n.label}</span>
                  {n.to === '/expenses' && pending > 0 && <b className="nav-count">{pending}</b>}
                </>
              )}
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="privacy-card">
            <ShieldCheck size={19} />
            <strong>Your data stays yours.</strong>
            <p>
              Saved on this device.
              <br />
              Available without a connection.
            </p>
            <Link to="/settings">
              Privacy & storage <ArrowUpRight size={13} />
            </Link>
          </div>
          <NavLink to="/settings" className="settings-nav">
            {({ isActive }) => (
              <>
                {isActive && <NavPill id="sidebar-pill" />}
                <Settings2 size={18} />
                Settings
              </>
            )}
          </NavLink>
          <div className="profile">
            <span className="profile-avatar">{settings.employee ? settings.employee[0] : 'Y'}</span>
            <div>
              <strong>{settings.employee || 'Your workspace'}</strong>
              <small>No account required</small>
            </div>
            <span className="version">v0.1</span>
          </div>
        </div>
      </aside>
      <div className="main-shell">
        <header className="topbar">
          <div className="breadcrumbs">
            <span>Workspace</span>
            <ChevronRight size={13} />
            <strong>{section}</strong>
          </div>
          <div className="topbar-right">
            {processing ? (
              <span className="processing">
                <LoaderCircle className="spin" size={14} />
                {processing}
              </span>
            ) : (
              <span className="local-status">
                <i />
                Local storage active
              </span>
            )}
            <span className="divider" />
            <button
              className="icon-button"
              onClick={() => void toggleTheme()}
              aria-label="Toggle color theme"
            >
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={settings.theme === 'dark' ? 'sun' : 'moon'}
                  style={{ display: 'grid' }}
                  initial={{ rotate: -90, scale: 0.5, opacity: 0 }}
                  animate={{ rotate: 0, scale: 1, opacity: 1 }}
                  exit={{ rotate: 90, scale: 0.5, opacity: 0 }}
                  transition={spring.bouncy}
                >
                  {settings.theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}
                </motion.span>
              </AnimatePresence>
            </button>
            <span className="top-avatar">{settings.employee ? settings.employee[0] : 'Y'}</span>
          </div>
        </header>
        {!desktop && (
          <div className="preview-banner">
            <ShieldCheck size={14} />
            Interface preview · Launch the desktop app to import and save local data.
          </div>
        )}
        <main>
          {error && (
            <div className="inline-error">
              {error}
              <button className="text-button" onClick={() => void refresh()}>
                Retry
              </button>
            </div>
          )}
          {loading ? (
            <PageSkeleton />
          ) : (
            <AnimatePresence
              mode="wait"
              initial={false}
              onExitComplete={() => window.scrollTo(0, 0)}
            >
              <PageTransition key={location.pathname}>
                <Routes location={location}>
                  <Route path="/" element={<Dashboard />} />
                  <Route path="/expenses" element={<Expenses />} />
                  <Route path="/expenses/:id" element={<ExpenseDetail />} />
                  <Route path="/import" element={<ImportReceipts />} />
                  <Route path="/claims" element={<Claims />} />
                  <Route path="/claims/:id" element={<ClaimDetail />} />
                  <Route path="/settings" element={<Settings />} />
                  <Route path="*" element={<Dashboard />} />
                </Routes>
              </PageTransition>
            </AnimatePresence>
          )}
        </main>
        <footer className="workspace-footer">
          <span>
            <ShieldCheck size={12} />
            Private by default. Productive anywhere.
          </span>
          <span>Folio · Local-first expense management</span>
        </footer>
      </div>
      <nav className="mobile-nav" aria-label="Primary">
        {[...navigation, { to: '/settings', label: 'Settings', icon: Settings2 }].map((n) => (
          <NavLink key={n.to} to={n.to} end={n.to === '/'} aria-label={n.label}>
            {({ isActive }) => (
              <>
                {isActive && <NavPill id="mobile-pill" />}
                <n.icon size={20} strokeWidth={1.8} />
                <span aria-hidden="true">{n.label === 'Import receipts' ? 'Import' : n.label}</span>
              </>
            )}
          </NavLink>
        ))}
      </nav>
    </div>
  );
}
export default function App() {
  return (
    <MotionConfig reducedMotion="user">
      <Providers>
        <HashRouter>
          <LayoutGroup>
            <Shell />
          </LayoutGroup>
        </HashRouter>
      </Providers>
    </MotionConfig>
  );
}
