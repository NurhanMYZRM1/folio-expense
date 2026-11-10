import { test, expect } from '@playwright/test';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge, installBridge } from '../bridge';
import type { CloudStatus } from '../../src/lib/ipc';

test('cloud settings verifies an emailed link, compares conflicts and pauses on sign-out', async ({
  page,
}) => {
  const root = await mkdtemp(path.join(tmpdir(), 'folio-cloud-ui-'));
  const bridge = new RustBridge(path.join(root, 'app'));
  const native = bridge.call.bind(bridge);
  let status: CloudStatus = {
    signedIn: false,
    email: null,
    pending: 0,
    conflicts: 0,
    lastSync: null,
    error: null,
  };
  let requests = 0;
  let verified = false;
  let resolved = false;
  // Authentication is deliberately stubbed: this UI test never sends an email,
  // consumes a real token or changes hosted financial data. Other commands use Rust.
  bridge.call = async <T>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    if (command === 'cloud_request_code') {
      requests++;
      return status as T;
    }
    if (command === 'cloud_verify_code') {
      verified =
        args.code ===
        'https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/verify?token=fixture&type=magiclink';
      status = { ...status, signedIn: true, email: String(args.email), conflicts: 1 };
      return status as T;
    }
    if (command === 'cloud_status' || command === 'cloud_sync') return status as T;
    if (command === 'cloud_conflicts')
      return (
        status.conflicts
          ? [
              {
                id: 'conflict-1',
                entity: 'expenses',
                local: { merchant_name: 'Local cafe', total_amount_minor: 1200 },
                remote: { data: { merchant_name: 'Remote cafe', total_amount_minor: 1300 } },
              },
            ]
          : []
      ) as T;
    if (command === 'cloud_resolve_conflict') {
      resolved = args.id === 'conflict-1' && args.choice === 'remote';
      status = { ...status, conflicts: 0 };
      return status as T;
    }
    if (command === 'cloud_sign_out') {
      status = { ...status, signedIn: false, email: null };
      return status as T;
    }
    return native<T>(command, args);
  };
  await installBridge(page, bridge, []);
  try {
    await page.goto('/#/settings');
    await page.getByRole('button', { name: 'Email me a sign-in link', exact: true }).click();
    expect(requests).toBe(1);
    const code = page.getByLabel('Sign-in code or email link', { exact: true });
    await code.fill(
      'https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/verify?token=fixture&type=magiclink',
    );
    await page.getByRole('button', { name: 'Sign in and enable sync', exact: true }).click();
    await expect(
      page.getByText('Connected as paragonxplays@gmail.com', { exact: true }),
    ).toBeVisible();
    expect(verified).toBe(true);
    await page.getByRole('button', { name: 'Compare versions', exact: true }).click();
    await expect(page.getByRole('cell', { name: 'Local cafe', exact: true })).toBeVisible();
    await expect(page.getByRole('cell', { name: 'Remote cafe', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Use cloud version', exact: true }).click();
    await expect(
      page.getByText('No local changes waiting to sync.', { exact: true }),
    ).toBeVisible();
    expect(resolved).toBe(true);
    await page.getByRole('button', { name: 'Sign out', exact: true }).click();
    await expect(
      page.getByRole('button', { name: 'Email me a sign-in link', exact: true }),
    ).toBeVisible();
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('on a phone only the native loop syncs automatically, and Sync now still syncs', async ({
  page,
}) => {
  const root = await mkdtemp(path.join(tmpdir(), 'folio-cloud-phone-'));
  const bridge = new RustBridge(path.join(root, 'app'));
  const native = bridge.call.bind(bridge);
  const status: CloudStatus = {
    signedIn: true,
    email: 'paragonxplays@gmail.com',
    pending: 0,
    conflicts: 0,
    lastSync: null,
    error: null,
  };
  const calls = { status: 0, sync: 0 };
  // Cloud commands are stubbed, so nothing here reaches the hosted project.
  bridge.call = async <T>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    if (command === 'cloud_status') {
      calls.status++;
      return status as T;
    }
    if (command === 'cloud_sync') {
      calls.sync++;
      return status as T;
    }
    return native<T>(command, args);
  };
  await installBridge(page, bridge, []);
  await page.addInitScript(() => {
    // The phone host's bridge (mobile/src/dom/host.tsx); its native layer owns the sync loop.
    (window as unknown as Record<string, unknown>).__folio = {
      invoke: async () => JSON.stringify({ result: { platform: 'ios', paymentAccess: false } }),
    };
  });
  try {
    await page.goto('/#/settings');
    await expect(
      page.getByText('Connected as paragonxplays@gmail.com', { exact: true }),
    ).toBeVisible();
    await page.waitForTimeout(300);
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await expect.poll(() => calls.status).toBeGreaterThan(1);
    expect(calls.sync).toBe(0);
    await page.getByRole('button', { name: 'Sync now', exact: true }).click();
    await expect.poll(() => calls.sync).toBe(1);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('a sign-in link or opened-link address can be pasted without another email', async ({
  page,
}) => {
  const root = await mkdtemp(path.join(tmpdir(), 'folio-cloud-paste-'));
  const bridge = new RustBridge(path.join(root, 'app'));
  const native = bridge.call.bind(bridge);
  let status: CloudStatus = {
    signedIn: false,
    email: null,
    pending: 0,
    conflicts: 0,
    lastSync: null,
    error: null,
  };
  let requests = 0;
  let pasted = '';
  // Authentication is stubbed: no email is sent and no hosted session is used.
  bridge.call = async <T>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    if (command === 'cloud_request_code') {
      requests++;
      return status as T;
    }
    if (command === 'cloud_verify_code') {
      pasted = String(args.code);
      status = { ...status, signedIn: true, email: String(args.email) };
      return status as T;
    }
    if (command === 'cloud_status' || command === 'cloud_sync') return status as T;
    return native<T>(command, args);
  };
  await installBridge(page, bridge, []);
  try {
    await page.goto('/#/settings');
    await page
      .getByRole('button', { name: 'I already have a sign-in link', exact: true })
      .click({ timeout: 5000 });
    const opened = 'http://localhost:3000/#access_token=a&refresh_token=r&type=signup';
    await page.getByLabel('Sign-in code or email link', { exact: true }).fill(opened);
    await page.getByRole('button', { name: 'Sign in and enable sync', exact: true }).click();
    await expect(
      page.getByText('Connected as paragonxplays@gmail.com', { exact: true }),
    ).toBeVisible();
    expect(requests).toBe(0);
    expect(pasted).toBe(opened);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});
