import { test, expect, type Page } from '@playwright/test';
import { mkdtemp, readFile, readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge } from '../bridge';

// Point at a built .app/www.bundle or exported DOM directory. These checks load
// Metro's actual output, which can fail even when the desktop Vite tests pass.
const bundleDirectory = process.env.FOLIO_DOM_BUNDLE;
test.skip(!bundleDirectory, 'Set FOLIO_DOM_BUNDLE to the built mobile DOM bundle directory.');

async function bundledPage(marker: string) {
  for (const name of await readdir(bundleDirectory!)) {
    if (!name.endsWith('.html')) continue;
    const html = await readFile(path.join(bundleDirectory!, name), 'utf8');
    const scripts = [...html.matchAll(/<script[^>]+src="([^"]+)"/g)];
    for (const [, source] of scripts) {
      const code = await readFile(path.resolve(bundleDirectory!, source), 'utf8');
      if (code.includes(marker)) return name;
    }
  }
  throw new Error(`No bundled screen contains ${JSON.stringify(marker)}`);
}

async function installNativeHost(page: Page, bridge: RustBridge, initialPath: string) {
  const navigations: string[] = [];
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.route('http://folio-dom.test/**', async (route) => {
    const relative = decodeURIComponent(new URL(route.request().url()).pathname).slice(1);
    const file = path.resolve(bundleDirectory!, relative);
    if (!file.startsWith(path.resolve(bundleDirectory!) + path.sep)) {
      await route.fulfill({ status: 403 });
      return;
    }
    await route.fulfill({ path: file });
  });
  await page.exposeFunction(
    '__folioNativeAction',
    async ({ actionId, args }: { actionId: string; args: string[] }) => {
      if (actionId === 'navigate') navigations.push(args[0]);
      if (actionId === 'invoke') {
        try {
          return JSON.stringify({ result: await bridge.call(args[0], JSON.parse(args[1])) });
        } catch (error) {
          return JSON.stringify({ error });
        }
      }
      return null;
    },
  );
  await page.addInitScript((initialPath) => {
    type Action = { uid: string; actionId: string; args: string[] };
    const native = window as unknown as {
      __folioNativeAction: (action: Action) => Promise<unknown>;
    };
    Object.assign(window, {
      ReactNativeWebView: {
        injectedObjectJson: () =>
          JSON.stringify({
            EXPO_DOM_HOST_OS: 'ios',
            initialProps: {
              names: ['invoke', 'pick', 'renderPdf', 'navigate', 'setTitle'],
              props: { path: initialPath, dataVersion: 0, syncTitle: true },
            },
          }),
        postMessage: (raw: string) => {
          const message = JSON.parse(raw) as { type: string; data: Action };
          if (message.type !== '$$native_action') return;
          void native.__folioNativeAction(message.data).then((result) => {
            window.dispatchEvent(
              new CustomEvent('$$dom_event', {
                detail: {
                  type: '$$native_action_result',
                  data: { ...message.data, result },
                },
              }),
            );
          });
        },
      },
    });
  }, initialPath);
  return { navigations, errors };
}

test('the bundled overview hands Open finance to the native router', async ({ page }) => {
  const bridge = new RustBridge(await mkdtemp(path.join(tmpdir(), 'folio-mobile-overview-')));
  try {
    const { navigations, errors } = await installNativeHost(page, bridge, '/');
    await page.goto(`http://folio-dom.test/${await bundledPage('Open finance')}`);
    await page.getByRole('link', { name: 'Open finance', exact: true }).click();
    await expect.poll(() => navigations).toEqual(['/finance']);
    expect(errors).toEqual([]);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('the Metro Finance bundle opens every Finance section without a runtime error', async ({
  page,
}) => {
  const bridge = new RustBridge(await mkdtemp(path.join(tmpdir(), 'folio-mobile-finance-')));
  try {
    const { errors } = await installNativeHost(page, bridge, '/finance');
    await page.goto(`http://folio-dom.test/${await bundledPage('Record transaction')}`);
    const navigation = page.getByRole('navigation', { name: 'Finance', exact: true });
    await expect(navigation).toBeVisible({ timeout: 5000 });
    for (const [label, title] of [
      ['Overview', 'Finance overview'],
      ['Accounts & goals', 'Accounts & goals'],
      ['Transactions', 'Transactions'],
      ['Budgets', 'Budgets'],
      ['Bills & subscriptions', 'Bills & subscriptions'],
      ['Reports', 'Monthly reports'],
      ['Business', 'Business tools'],
      ['Tax helpers', 'Malaysian tax helpers'],
    ]) {
      await navigation.getByRole('link', { name: label, exact: true }).click();
      await expect(page.locator('.page-heading h1')).toHaveText(title);
      expect(errors).toEqual([]);
    }
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});
