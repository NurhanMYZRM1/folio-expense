/** Shares a foreground sync request so timer, reconnect and manual retries cannot overlap. */
export function runSingleFlight(
  slot: { current: Promise<void> | null },
  task: () => Promise<void>,
): Promise<void> {
  if (slot.current) return slot.current;
  const next = Promise.resolve().then(task);
  slot.current = next;
  void next.then(
    () => {
      if (slot.current === next) slot.current = null;
    },
    () => {
      if (slot.current === next) slot.current = null;
    },
  );
  return next;
}

/** Syncs in the foreground and once on departure; cleanup removes all signals. */
export function startForegroundSync(sync: () => Promise<void>): () => void {
  let active = true;
  const run = () => {
    if (active && document.visibilityState !== 'hidden' && navigator.onLine !== false) void sync();
  };
  const visibilityChanged = () => {
    if (active && navigator.onLine !== false) void sync();
  };
  run();
  const timer = window.setInterval(run, 20_000);
  window.addEventListener('online', run);
  window.addEventListener('focus', run);
  document.addEventListener('visibilitychange', visibilityChanged);
  return () => {
    active = false;
    window.clearInterval(timer);
    window.removeEventListener('online', run);
    window.removeEventListener('focus', run);
    document.removeEventListener('visibilitychange', visibilityChanged);
  };
}
