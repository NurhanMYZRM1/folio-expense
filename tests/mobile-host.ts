import { type Page } from '@playwright/test';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { RustBridge } from './bridge';

// Point at a built .app/www.bundle or exported DOM directory. These helpers load
// Metro's actual output, which can fail even when the desktop Vite tests pass.
export const bundleDirectory = process.env.FOLIO_DOM_BUNDLE;

/** Replaces the core's answer for one command, to show how a screen handles it. */
export type Intercept = (command: string, args: unknown) => { error: unknown } | undefined;

export async function bundledPage(marker: string) {
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

export async function installNativeHost(
  page: Page,
  bridge: RustBridge,
  initialPath: string,
  intercept?: Intercept,
) {
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
        const replaced = intercept?.(args[0], JSON.parse(args[1]));
        if (replaced) return JSON.stringify(replaced);
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
