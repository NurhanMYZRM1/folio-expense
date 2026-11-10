import { test, expect } from '@playwright/test';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge } from '../bridge';
import { bundleDirectory, bundledPage, installNativeHost } from '../mobile-host';

test.skip(!bundleDirectory, 'Set FOLIO_DOM_BUNDLE to the built mobile DOM bundle directory.');

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
