import { test, expect, type Locator, type Page } from '@playwright/test';
import { mkdir, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge } from '../bridge';
import { bundleDirectory, bundledPage, installNativeHost, type Intercept } from '../mobile-host';

// The phone's shared screens, loaded from Metro's real output at phone widths.
// This is a browser check of the web layer; the native banner and the iOS
// keyboard are checked on a simulator or device (docs/VERIFICATION.md).
test.skip(!bundleDirectory, 'Set FOLIO_DOM_BUNDLE to the built mobile DOM bundle directory.');

const WIDTHS = [320, 375, 390, 430];
const HEIGHT = 700;
/** Roughly what is left above an iPhone keyboard. */
const KEYBOARD_HEIGHT = 330;
const UNBROKEN = `${'IMG_20261110_RECEIPT_'.repeat(8)}.jpeg`;
const shots = process.env.FOLIO_LAYOUT_SHOTS;

async function shot(page: Page, name: string) {
  if (!shots) return;
  await mkdir(shots, { recursive: true });
  await page.screenshot({ path: path.join(shots, `${name}.png`) });
}
async function expectNoSidewaysPage(page: Page) {
  const extra = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect.soft(extra, 'the page must not scroll sideways').toBeLessThanOrEqual(0);
}
/** What is visible: the window, less whatever the keyboard covers. */
const visibleHeight = (page: Page) =>
  page.evaluate(() => window.visualViewport?.height ?? window.innerHeight);
/**
 * Opens the iOS keyboard as WKWebView reports it: the page keeps its size and
 * only the visual viewport shrinks.
 */
const openKeyboard = (page: Page) =>
  page.evaluate(
    (height) => (window as unknown as { __keyboard: (h: number) => void }).__keyboard(height),
    KEYBOARD_HEIGHT,
  );
/** The whole element lies inside the visible screen. */
async function expectOnScreen(page: Page, target: Locator, what: string) {
  const box = (await target.boundingBox())!;
  const view = { width: page.viewportSize()!.width, height: await visibleHeight(page) };
  expect(box, `${what} is rendered`).not.toBeNull();
  expect.soft(box.x, `${what}: left edge`).toBeGreaterThanOrEqual(-0.5);
  expect.soft(box.y, `${what}: top edge`).toBeGreaterThanOrEqual(-0.5);
  expect.soft(box.x + box.width, `${what}: right edge`).toBeLessThanOrEqual(view.width + 0.5);
  expect.soft(box.y + box.height, `${what}: bottom edge`).toBeLessThanOrEqual(view.height + 0.5);
}
/** The control can be brought fully into the visible screen and pressed. */
async function expectReachable(page: Page, control: Locator, what: string) {
  await control.scrollIntoViewIfNeeded();
  await expectOnScreen(page, control, what);
  await expect
    .soft(
      control.click({ trial: true, timeout: 1500 }).then(
        () => 'can be pressed',
        () => 'is covered or off screen',
      ),
      what,
    )
    .resolves.toBe('can be pressed');
}
/** Stands in for large accessibility text inside a dialog or notice. */
const largeText = (page: Page) =>
  page.addStyleTag({
    content:
      '.modal, .modal *, .toast, .toast * { font-size: 26px !important; line-height: 1.45 !important; }',
  });

async function open(
  page: Page,
  width: number,
  marker: string,
  route: string,
  prepare: (bridge: RustBridge) => Promise<string | void>,
  intercept?: Intercept,
) {
  const bridge = new RustBridge(await mkdtemp(path.join(tmpdir(), 'folio-mobile-layout-')));
  await page.setViewportSize({ width, height: HEIGHT });
  await page.emulateMedia({ colorScheme: width % 20 === 0 ? 'dark' : 'light' });
  const id = (await prepare(bridge)) ?? '';
  const host = await installNativeHost(page, bridge, route.replace(':id', id), intercept);
  await page.addInitScript(() => {
    const view = Object.assign(new EventTarget(), {
      width: window.innerWidth,
      height: window.innerHeight,
      offsetTop: 0,
      offsetLeft: 0,
      scale: 1,
    });
    Object.defineProperty(window, 'visualViewport', { value: view });
    Object.assign(window, {
      __keyboard: (height: number) => {
        view.height = height;
        view.dispatchEvent(new Event('resize'));
      },
    });
  });
  await page.goto(`http://folio-dom.test/${await bundledPage(marker)}`);
  return { bridge, ...host };
}
async function longExpense(bridge: RustBridge) {
  const created = await bridge.call<{ id: string; version: number }>('create_expense');
  await bridge.call('edit_expense', {
    edit: {
      id: created.id,
      version: created.version,
      occurredAt: '2026-11-01',
      merchantName: UNBROKEN,
      premises: null,
      totalAmountMinor: 123456,
      taxAmountMinor: 0,
      currency: 'MYR',
      category: 'Meals',
      description: `Imported from ${UNBROKEN}`,
      markReady: false,
    },
  });
  return created.id;
}

for (const width of WIDTHS) {
  test(`${width}px: the delete prompt and its actions stay on screen, also above the keyboard`, async ({
    page,
  }) => {
    const { bridge, errors } = await open(
      page,
      width,
      'Delete this expense?',
      '/expenses/:id',
      longExpense,
    );
    try {
      await expect(page.getByRole('button', { name: 'Delete expense' })).toBeVisible();
      await expectNoSidewaysPage(page);
      await page.getByRole('button', { name: 'Delete expense' }).click();
      const dialog = page.getByRole('alertdialog');
      await expect(dialog).toBeVisible();
      const cancel = dialog.getByRole('button', { name: 'Cancel', exact: true });
      const confirm = dialog.getByRole('button', { name: 'Delete permanently' });
      await expect(cancel).toBeFocused();
      await page.waitForTimeout(400); // The entrance animation settles.
      await expectOnScreen(page, dialog, 'dialog');
      await expectReachable(page, cancel, 'Cancel');
      await expectReachable(page, confirm, 'Delete permanently');
      // The screen behind is out of reach of touch, keyboard and VoiceOver.
      await expect.soft(page.locator('main.mobile-page')).toHaveJSProperty('inert', true);
      await shot(page, `delete-${width}`);

      // Large text and a keyboard leave far less room than the content needs.
      await largeText(page);
      await openKeyboard(page);
      await page.waitForTimeout(200);
      await expectOnScreen(page, dialog, 'dialog with large text above the keyboard');
      await expectReachable(page, confirm, 'Delete permanently with large text');
      await expectReachable(page, cancel, 'Cancel with large text');
      await expectNoSidewaysPage(page);
      await shot(page, `delete-large-keyboard-${width}`);

      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);
      await expect.soft(page.locator('main.mobile-page')).toHaveJSProperty('inert', false);
      expect(errors).toEqual([]);
    } finally {
      await page.goto('about:blank');
      await bridge.close();
    }
  });

  test(`${width}px: a long form dialog scrolls and keeps Save reachable`, async ({ page }) => {
    const { bridge, errors } = await open(page, width, 'Record transaction', '/finance', async () => {});
    try {
      await page.getByRole('link', { name: 'Accounts & goals', exact: true }).click();
      await page.getByRole('button', { name: 'New account' }).first().click();
      const dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      await page.waitForTimeout(400);
      await expectOnScreen(page, dialog, 'form dialog');
      const save = dialog.locator('button[type="submit"]');
      await expectReachable(page, save, 'Save');
      await dialog.locator('input').first().focus();
      await openKeyboard(page);
      await page.waitForTimeout(200);
      await expectOnScreen(page, dialog, 'form dialog above the keyboard');
      await expectReachable(page, save, 'Save above the keyboard');
      await expectReachable(page, dialog.getByRole('button', { name: 'Cancel' }), 'Cancel');
      await expectNoSidewaysPage(page);
      await shot(page, `form-keyboard-${width}`);
      expect(errors).toEqual([]);
    } finally {
      await page.goto('about:blank');
      await bridge.close();
    }
  });

  test(`${width}px: a long error notice wraps and can be dismissed`, async ({ page }) => {
    const { bridge } = await open(
      page,
      width,
      'Delete this expense?',
      '/expenses/:id',
      longExpense,
      (command) =>
        command === 'delete_expenses'
          ? {
              error: {
                code: 'StorageError',
                message: `Could not remove ${UNBROKEN} from /var/mobile/Containers/Data/Application/${UNBROKEN}. Check available space and try again.`,
              },
            }
          : undefined,
    );
    try {
      await page.getByRole('button', { name: 'Delete expense' }).click();
      await page.getByRole('button', { name: 'Delete permanently' }).click();
      const problem = page.locator('.toast.error, .inline-error').first();
      await expect(problem).toBeVisible();
      await page.waitForTimeout(400);
      await expectNoSidewaysPage(page);
      const box = (await problem.boundingBox())!;
      expect(box.x).toBeGreaterThanOrEqual(-0.5);
      expect(box.x + box.width).toBeLessThanOrEqual(width + 0.5);
      await shot(page, `long-error-${width}`);
      await largeText(page);
      await expectNoSidewaysPage(page);
      await openKeyboard(page);
      await page.waitForTimeout(200);
      const dismiss = page.getByRole('button', { name: 'Dismiss notification' });
      if (await dismiss.count()) await expectReachable(page, dismiss.first(), 'Dismiss');
      await shot(page, `long-error-large-keyboard-${width}`);
    } finally {
      await page.goto('about:blank');
      await bridge.close();
    }
  });

  test(`${width}px: no shared screen scrolls sideways with long names`, async ({ context }) => {
    for (const [marker, route] of [
      ['Open finance', '/'],
      ['Preferences are up to date.', '/settings'],
      ['Record transaction', '/finance'],
    ] as const) {
      const page = await context.newPage();
      const { bridge } = await open(page, width, marker, route, async (b) => {
        await longExpense(b);
        await b.call('create_claim', { title: UNBROKEN, currency: 'MYR' });
      });
      try {
        await expect(page.locator('main.mobile-page')).toBeVisible();
        await page.waitForTimeout(500);
        const widest = await page.evaluate(() => {
          const limit = document.documentElement.clientWidth;
          const over = [...document.querySelectorAll<HTMLElement>('main *')]
            .filter((el) => el.getBoundingClientRect().right > limit + 0.5)
            // Tables and tab strips scroll sideways inside their own box by design.
            .filter((el) => {
              for (let p = el.parentElement; p && p !== document.body; p = p.parentElement)
                if (/auto|scroll/.test(getComputedStyle(p).overflowX)) return false;
              return true;
            });
          const el = over.at(-1);
          return el ? `${el.tagName.toLowerCase()}.${el.className}` : '';
        });
        expect.soft(widest, `${route}: nothing reaches past the right edge`).toBe('');
        await expectNoSidewaysPage(page);
        await shot(page, `screen-${route.replace(/\W/g, '') || 'overview'}-${width}`);
      } finally {
        await page.close();
        await bridge.close();
      }
    }
  });
}
