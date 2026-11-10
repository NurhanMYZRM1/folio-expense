import { afterEach, describe, expect, it, vi } from 'vitest';
import { runSingleFlight, startForegroundSync } from './cloudSync';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('cloud sync foreground lifecycle', () => {
  it('shares concurrent requests and permits another run after failure', async () => {
    let reject!: (error: Error) => void;
    const task = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<void>((_resolve, fail) => {
            reject = fail;
          }),
      )
      .mockResolvedValue(undefined);
    const slot = { current: null as Promise<void> | null };
    const run = () => runSingleFlight(slot, task);
    const first = run();
    expect(run()).toBe(first);
    await Promise.resolve();
    expect(task).toHaveBeenCalledTimes(1);
    reject(new Error('Offline'));
    await expect(first).rejects.toThrow('Offline');
    await run();
    expect(task).toHaveBeenCalledTimes(2);
  });

  it('pauses in hidden/offline windows, resumes on signals, and cleans up', async () => {
    vi.useFakeTimers();
    const browser = new EventTarget();
    const documentEvents = new EventTarget();
    const doc = Object.assign(documentEvents, { visibilityState: 'visible' });
    const net = { onLine: true };
    vi.stubGlobal('window', Object.assign(browser, { setInterval, clearInterval }));
    vi.stubGlobal('document', doc);
    vi.stubGlobal('navigator', net);
    const sync = vi.fn().mockResolvedValue(undefined);
    const stop = startForegroundSync(sync);
    expect(sync).toHaveBeenCalledTimes(1);
    doc.visibilityState = 'hidden';
    await vi.advanceTimersByTimeAsync(20_000);
    browser.dispatchEvent(new Event('focus'));
    expect(sync).toHaveBeenCalledTimes(1);
    doc.visibilityState = 'visible';
    net.onLine = false;
    doc.dispatchEvent(new Event('visibilitychange'));
    expect(sync).toHaveBeenCalledTimes(1);
    net.onLine = true;
    browser.dispatchEvent(new Event('online'));
    expect(sync).toHaveBeenCalledTimes(2);
    stop();
    await vi.advanceTimersByTimeAsync(40_000);
    browser.dispatchEvent(new Event('online'));
    expect(sync).toHaveBeenCalledTimes(2);
  });

  it('syncs once as the window is hidden so edits made just before leave the device', async () => {
    vi.useFakeTimers();
    const doc = Object.assign(new EventTarget(), { visibilityState: 'visible' });
    const net = { onLine: true };
    vi.stubGlobal('window', Object.assign(new EventTarget(), { setInterval, clearInterval }));
    vi.stubGlobal('document', doc);
    vi.stubGlobal('navigator', net);
    const sync = vi.fn().mockResolvedValue(undefined);
    const stop = startForegroundSync(sync);
    doc.visibilityState = 'hidden';
    doc.dispatchEvent(new Event('visibilitychange'));
    expect(sync).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(40_000);
    expect(sync).toHaveBeenCalledTimes(2);
    net.onLine = false;
    doc.dispatchEvent(new Event('visibilitychange'));
    expect(sync).toHaveBeenCalledTimes(2);
    stop();
    net.onLine = true;
    doc.dispatchEvent(new Event('visibilitychange'));
    expect(sync).toHaveBeenCalledTimes(2);
  });
});
