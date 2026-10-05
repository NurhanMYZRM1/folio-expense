// Runs one of the desktop app's screens inside a DOM component: the shared
// Providers and react-router hooks work unchanged; leaving the screen's route
// hands navigation to the native Expo Router stack.
import { useEffect, useRef, type ReactNode } from "react";
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { Providers, useWorkspace } from "@folio/app/providers";
import type { FolioBridge } from "./bridge";

export type NavigateMode = "push" | "replace";

export interface FolioHostProps extends FolioBridge {
  navigate: (path: string, mode: NavigateMode) => Promise<void>;
  /** The screen's heading, shown in the native navigation bar. */
  setTitle: (title: string) => Promise<void>;
  /** Function props are always proxied in the webview, so presence is signalled separately. */
  syncTitle: boolean;
  /** Path and query of the native route, e.g. "/expenses/abc". */
  path: string;
  /** Bumped by the native side whenever the core's records change. */
  dataVersion: number;
  dom?: import("expo/dom").DOMProps;
}

function installBridge(live: { current: FolioHostProps }) {
  const w = window as unknown as { __folio?: FolioBridge };
  if (w.__folio) return;
  w.__folio = {
    invoke: (command, args) => live.current.invoke(command, args),
    pick: (options) => live.current.pick(options),
    renderPdf: (relativePath) => live.current.renderPdf(relativePath),
  };
}

function RefreshOnChange({ version }: { version: number }) {
  const { refresh } = useWorkspace();
  const first = useRef(version);
  useEffect(() => {
    if (version !== first.current) void refresh();
  }, [version, refresh]);
  return null;
}

/** Remembers the last location this screen owns, to return to after handing a link to native. */
function Track({ last, children }: { last: { current: string }; children: ReactNode }) {
  const location = useLocation();
  last.current = location.pathname + location.search;
  return <>{children}</>;
}

function Leave({ last, live }: { last: { current: string }; live: { current: FolioHostProps } }) {
  const location = useLocation();
  const navigate = useNavigate();
  useEffect(() => {
    const to = location.pathname + location.search;
    if (to === last.current) return;
    void live.current.navigate(to, "push");
    navigate(last.current, { replace: true });
  }, [location, navigate, last, live]);
  return null;
}

function TitleSync({ live, enabled }: { live: { current: FolioHostProps }; enabled: boolean }) {
  useEffect(() => {
    if (!enabled) return;
    let shown = "";
    const sync = () => {
      const text = document.querySelector(".mobile-page .page-heading h1, .mobile-page h1")?.textContent?.trim() ?? "";
      if (text && text !== shown) {
        shown = text;
        void live.current.setTitle(text);
      }
    };
    const observer = new MutationObserver(sync);
    observer.observe(document.body, { childList: true, subtree: true, characterData: true });
    sync();
    return () => observer.disconnect();
  }, [enabled, live]);
  return null;
}

export function FolioHost({ pattern, children, ...props }: FolioHostProps & { pattern: string; children: ReactNode }) {
  const live = useRef(props);
  live.current = props;
  const last = useRef(props.path);
  if (typeof window !== "undefined") installBridge(live);

  return (
    <MemoryRouter initialEntries={[props.path]}>
      <Providers>
        <RefreshOnChange version={props.dataVersion} />
        <TitleSync live={live} enabled={props.syncTitle} />
        <Routes>
          <Route
            path={pattern}
            element={
              <Track last={last}>
                <main className="mobile-page">{children}</main>
              </Track>
            }
          />
          <Route path="*" element={<Leave last={last} live={live} />} />
        </Routes>
      </Providers>
    </MemoryRouter>
  );
}
