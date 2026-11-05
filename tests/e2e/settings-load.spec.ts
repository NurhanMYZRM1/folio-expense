import { test, expect } from '@playwright/test';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge, installBridge } from '../bridge';
import type { Settings } from '../../src/bindings/generated';

test('Settings waits for the current load before initializing its editable fields', async ({
  page,
}) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-settings-load-'));
  const bridge = new RustBridge(path.join(directory, 'app'));
  const settings = await bridge.call<Settings>('get_settings');
  await bridge.call('save_settings', { settings: { ...settings, company: 'Saved organization' } });
  let reads = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const proxy = {
    call: async <T>(command: string, args: Record<string, unknown>) => {
      const result = await bridge.call<T>(command, args);
      // React StrictMode starts a newer refresh while the first is pending.
      // Keep the newer settings response in flight after the stale one ends.
      if (command === 'get_settings' && ++reads === 2) await held;
      return result;
    },
  } as RustBridge;
  await installBridge(page, proxy, []);
  await page.goto('about:blank');
  try {
    await page.goto('/#/settings');
    await expect.poll(() => reads).toBe(2);
    await page.evaluate(
      () =>
        new Promise<void>((resolve) => {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
        }),
    );
    const company = page.getByLabel('Organization or personal label', { exact: true });
    await expect(company).toHaveCount(0);
    release();
    await expect(company).toHaveValue('Saved organization');
    await expect(page.getByText('Preferences are up to date.', { exact: true })).toBeVisible();
  } finally {
    release();
    await page.goto('about:blank');
    await bridge.close();
  }
});
