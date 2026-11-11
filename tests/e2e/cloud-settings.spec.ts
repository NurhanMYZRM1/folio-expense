import { test, expect } from '@playwright/test';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge, installBridge } from '../bridge';
import type { CloudStatus } from '../../src/lib/ipc';

const OWNER = 'owner@example.com';
const signedOut: CloudStatus = {
  signedIn: false,
  linked: false,
  email: null,
  providers: [],
  pending: 0,
  conflicts: 0,
  lastSync: null,
  error: null,
  state: 'local',
  failure: null,
  retryAt: null,
  requestId: null,
};
const signedIn = (email: string, more: Partial<CloudStatus> = {}): CloudStatus => ({
  ...signedOut,
  signedIn: true,
  linked: true,
  email,
  providers: ['email'],
  state: 'upToDate',
  ...more,
});

test('cloud settings verifies an emailed link, compares conflicts and pauses on sign-out', async ({
  page,
}) => {
  const root = await mkdtemp(path.join(tmpdir(), 'folio-cloud-ui-'));
  const bridge = new RustBridge(path.join(root, 'app'));
  const native = bridge.call.bind(bridge);
  let status: CloudStatus = signedOut;
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
      status = signedIn(String(args.email), { conflicts: 1, state: 'needsAttention' });
      return { outcome: 'signedIn', status } as T;
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
      status = { ...status, conflicts: 0, state: 'upToDate' };
      return status as T;
    }
    if (command === 'cloud_sign_out') {
      status = { ...status, signedIn: false, state: 'signedOut' };
      return status as T;
    }
    return native<T>(command, args);
  };
  await installBridge(page, bridge, []);
  try {
    await page.goto('/#/settings');
    await page.getByLabel('Or sign in by email', { exact: true }).fill(OWNER);
    await page.getByRole('button', { name: 'Email me a sign-in link', exact: true }).click();
    expect(requests).toBe(1);
    const code = page.getByLabel('Sign-in code or email link', { exact: true });
    await code.fill(
      'https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/verify?token=fixture&type=magiclink',
    );
    await page.getByRole('button', { name: 'Sign in and enable sync', exact: true }).click();
    await expect(page.getByText('Connected as owner@example.com', { exact: true })).toBeVisible();
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
  const status = signedIn(OWNER);
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
    await expect(page.getByText('Connected as owner@example.com', { exact: true })).toBeVisible();
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
  let status: CloudStatus = signedOut;
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
      status = signedIn(String(args.email));
      return { outcome: 'signedIn', status } as T;
    }
    if (command === 'cloud_status' || command === 'cloud_sync') return status as T;
    return native<T>(command, args);
  };
  await installBridge(page, bridge, []);
  try {
    await page.goto('/#/settings');
    await page.getByLabel('Or sign in by email', { exact: true }).fill(OWNER);
    await page
      .getByRole('button', { name: 'I already have a sign-in link', exact: true })
      .click({ timeout: 5000 });
    const opened = 'http://localhost:3000/#access_token=a&refresh_token=r&type=signup';
    await page.getByLabel('Sign-in code or email link', { exact: true }).fill(opened);
    await page.getByRole('button', { name: 'Sign in and enable sync', exact: true }).click();
    await expect(page.getByText('Connected as owner@example.com', { exact: true })).toBeVisible();
    expect(requests).toBe(0);
    expect(pasted).toBe(opened);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('Google sign-in asks before this device’s records join the account', async ({ page }) => {
  const root = await mkdtemp(path.join(tmpdir(), 'folio-cloud-provider-'));
  const bridge = new RustBridge(path.join(root, 'app'));
  const native = bridge.call.bind(bridge);
  let status: CloudStatus = signedOut;
  const seen: string[] = [];
  // The provider and the cloud are stubbed: this checks the screens, not a real sign-in.
  bridge.call = async <T>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    if (command.startsWith('cloud_')) seen.push(`${command} ${JSON.stringify(args)}`);
    if (command === 'cloud_providers') return ['google'] as T;
    if (command === 'cloud_oauth_start') return null as T;
    if (command === 'cloud_oauth_cancel') return null as T;
    if (command === 'cloud_sign_in') return { outcome: 'chooseData', email: OWNER } as T;
    if (command === 'cloud_adopt') {
      if (args.choice === 'cancel') return { outcome: 'cancelled', status } as T;
      status = signedIn(OWNER, { providers: ['email', 'google'] });
      return { outcome: 'signedIn', status } as T;
    }
    if (command === 'cloud_status' || command === 'cloud_sync') return status as T;
    if (command === 'cloud_diagnose')
      return {
        environment: 'test',
        project: 'project.example',
        keyKind: 'publishable',
        callback: 'expenseapp://auth/callback',
        reachable: 'ok',
        authenticated: 'ok',
        pendingFiles: 0,
        status,
      } as T;
    return native<T>(command, args);
  };
  await installBridge(page, bridge, []);
  try {
    await page.goto('/#/settings');
    // The desktop hands off to the system browser and can be cancelled while waiting.
    await page.getByRole('button', { name: 'Continue with Google', exact: true }).click();
    await expect(page.getByText('Finish signing in in your browser')).toBeVisible();
    expect(seen).toContain('cloud_oauth_start {"provider":"google","link":false}');
    // Apple is switched off for this project, so it is not offered.
    await expect(page.getByRole('button', { name: 'Continue with Apple' })).toHaveCount(0);
    await page.getByRole('button', { name: 'Cancel sign-in', exact: true }).click();
    await expect.poll(() => seen.some((c) => c.startsWith('cloud_oauth_cancel'))).toBe(true);
    await expect(
      page.getByRole('button', { name: 'Continue with Google', exact: true }),
    ).toBeEnabled();

    // A sign-in on a device that already has records stops for a decision.
    seen.length = 0;
    await page.getByLabel('Or sign in by email', { exact: true }).fill(OWNER);
    await page.getByRole('button', { name: 'I already have a sign-in link', exact: true }).click();
    bridge.call = (
      (inner) =>
      async <T>(command: string, args: Record<string, unknown> = {}): Promise<T> =>
        command === 'cloud_verify_code'
          ? ({ outcome: 'chooseData', email: OWNER } as T)
          : inner<T>(command, args)
    )(bridge.call.bind(bridge));
    await page.getByLabel('Sign-in code or email link', { exact: true }).fill('123456');
    await page.getByRole('button', { name: 'Sign in and enable sync', exact: true }).click();
    const dialog = page.getByRole('alertdialog');
    await expect(dialog).toContainText(`You are signing in as ${OWNER}`);
    await expect(page.getByText(`Connected as ${OWNER}`)).toHaveCount(0);
    await dialog.getByRole('button', { name: 'Add to this account', exact: true }).click();
    await expect(page.getByText(`Connected as ${OWNER}`, { exact: true })).toBeVisible();
    expect(seen).toContain('cloud_adopt {"choice":"join"}');
    await expect(page.getByText('Sign-in methods: Email, Google.', { exact: false })).toBeVisible();
    // Neither a method the account already has nor one the project lacks is offered.
    await expect(page.getByRole('button', { name: 'Add Apple sign-in' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Add Google sign-in' })).toHaveCount(0);

    await page.getByText('Connection details', { exact: true }).click();
    await expect(page.getByText('Signed-in sync', { exact: true })).toBeVisible();
    await expect(page.locator('.cloud-details')).not.toContainText('sb_publishable');
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});
