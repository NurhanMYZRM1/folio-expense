import { test, expect } from '@playwright/test';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge, installBridge } from '../bridge';
import type { Account, Workspace } from '../../src/bindings/generated';

test('opening balance keeps keyboard focus when creating and editing an account', async ({
  page,
}) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-account-focus-'));
  const bridge = new RustBridge(path.join(directory, 'app'));
  await installBridge(page, bridge, []);
  await page.goto('about:blank');
  try {
    await page.goto('/#/finance/accounts');
    const newAccount = page.getByRole('button', { name: 'New account', exact: true });
    await newAccount.click();
    const name = page.getByLabel('Account name', { exact: true });
    const opening = page.getByLabel('Opening balance', { exact: true });
    await name.fill('Focus test bank');
    await opening.fill('');
    // A normal fill replaces the value in one event and hid the focus bug.
    // Pace actual keystrokes so each render/focus effect can run between keys.
    await opening.pressSequentially('123.45', { delay: 80 });
    await expect(opening).toBeFocused();
    await expect(opening).toHaveValue('123.45');
    await expect(name).toHaveValue('Focus test bank');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByText('MYR 123.45', { exact: true })).toBeVisible();
    const edit = page.getByRole('button', { name: 'Edit Focus test bank', exact: true });
    await edit.click();
    await opening.fill('');
    await opening.pressSequentially('-98.76', { delay: 80 });
    await expect(opening).toBeFocused();
    await expect(opening).toHaveValue('-98.76');
    await expect(name).toHaveValue('Focus test bank');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await edit.click();
    await expect(opening).toHaveValue('-98.76');
    await opening.click();
    await opening.press('Escape');
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(edit).toBeFocused();
    await page.reload();
    await expect(page.getByText('−MYR 98.76', { exact: true })).toBeVisible();
    const [workspace] = await bridge.call<Workspace[]>('list_workspaces');
    const [account] = await bridge.call<Account[]>('list_accounts', { workspaceId: workspace.id });
    expect(account.openingBalanceMinor).toBe(-9876);
    expect(account.name).toBe('Focus test bank');
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});
