import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { createInterface } from 'node:readline';
import path from 'node:path';
import type { Page } from '@playwright/test';
export class RustBridge {
  private process: ChildProcessWithoutNullStreams;
  private sequence = 0;
  private pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: unknown) => void }
  >();
  constructor(readonly directory: string) {
    this.process = spawn(
      path.resolve(
        `src-tauri/target/debug/test-driver${process.platform === 'win32' ? '.exe' : ''}`,
      ),
      [directory],
    );
    createInterface({ input: this.process.stdout }).on('line', (line) => {
      const response = JSON.parse(line);
      const waiting = this.pending.get(response.requestId);
      if (!waiting) return;
      this.pending.delete(response.requestId);
      if (response.error) waiting.reject(response.error);
      else waiting.resolve(response.result);
    });
    this.process.stderr.on('data', (bytes) => process.stderr.write(bytes));
    this.process.on('exit', () => {
      for (const p of this.pending.values()) p.reject(new Error('Rust test driver exited'));
      this.pending.clear();
    });
  }
  call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    const requestId = ++this.sequence;
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, { resolve: (value) => resolve(value as T), reject });
      this.process.stdin.write(JSON.stringify({ requestId, command, args }) + '\n');
    });
  }
  close(): Promise<void> {
    return new Promise((resolve) => {
      this.process.once('exit', () => resolve());
      this.process.stdin.end();
    });
  }
}
export async function installBridge(page: Page, bridge: RustBridge, receiptPaths: string[]) {
  await page.exposeFunction(
    '__folioInvoke',
    async (command: string, args: Record<string, unknown>) => {
      if (command === 'plugin:dialog|open') return receiptPaths;
      if (command === 'plugin:deep-link|get_current') return [];
      if (command.startsWith('plugin:event|')) return 1;
      return bridge.call(command, args);
    },
  );
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.isTauri = true;
    w.__FOLIO_TEST__ = true;
    let id = 0;
    w.__TAURI_INTERNALS__ = {
      invoke: w.__folioInvoke,
      transformCallback: () => ++id,
      unregisterCallback: () => {},
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
    };
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  });
}
