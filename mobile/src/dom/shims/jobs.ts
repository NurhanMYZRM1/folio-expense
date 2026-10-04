// Background jobs run once, natively (src/core/jobs.ts), not in every screen's webview.
export function startJobRunner(): () => void {
  return () => undefined;
}
