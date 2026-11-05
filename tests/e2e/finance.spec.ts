import { test, expect } from '@playwright/test';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { RustBridge, installBridge } from '../bridge';
test.use({ actionTimeout: 10_000, screenshot: 'only-on-failure' });

import type { Account, Bill, TransactionPage, Workspace } from '../../src/bindings/generated';

test('personal finance persists income, transfers, budgets and a recurring bill payment', async ({
  page,
}, testInfo) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-finance-ui-'));
  let bridge = new RustBridge(path.join(directory, 'app'));
  const proxy = {
    call: <T>(cmd: string, args: Record<string, unknown>) => bridge.call<T>(cmd, args),
  } as RustBridge;
  await installBridge(page, proxy, []);
  await page.goto('about:blank');
  try {
    await page.goto('/#/finance/accounts');
    await page.getByRole('button', { name: 'New account', exact: true }).click();
    await page.getByLabel('Account name', { exact: true }).fill('Daily bank');
    await page.getByLabel('Opening balance', { exact: true }).fill('1000');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByText('MYR 1,000.00', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'New account', exact: true }).click();
    await page.getByLabel('Account name', { exact: true }).fill('Holiday savings');
    await page.getByRole('combobox', { name: 'Account type', exact: true }).selectOption('savings');
    await page.getByLabel('Savings target', { exact: true }).fill('500');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    const [workspace] = await bridge.call<Workspace[]>('list_workspaces');
    const accounts = await bridge.call<Account[]>('list_accounts', { workspaceId: workspace.id });
    const bank = accounts.find((a) => a.name === 'Daily bank')!,
      savings = accounts.find((a) => a.name === 'Holiday savings')!;
    await page.goto('/#/finance/transactions');
    await page.getByRole('button', { name: 'New transaction', exact: true }).click();
    await page.getByRole('combobox', { name: 'Transaction kind', exact: true }).selectOption('income');
    await page.getByRole('combobox', { name: 'Transaction account', exact: true }).selectOption(bank.id);
    await page.getByLabel('Amount (MYR)', { exact: true }).fill('200');
    await page.getByLabel('Payee / payer', { exact: true }).fill('Salary');
    await page.getByRole('button', { name: 'Confirm transaction', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'New transaction', exact: true }).click();
    await page.getByRole('combobox', { name: 'Transaction account', exact: true }).selectOption(bank.id);
    await page.getByLabel('Amount (MYR)', { exact: true }).fill('25');
    await page.getByLabel('Payee / payer', { exact: true }).fill('Lunch');
    await page.getByRole('button', { name: 'Confirm transaction', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'New transaction', exact: true }).click();
    await page.getByRole('combobox', { name: 'Transaction kind', exact: true }).selectOption('transfer');
    await page.getByRole('combobox', { name: 'From account', exact: true }).selectOption(bank.id);
    await page.getByRole('combobox', { name: 'To account', exact: true }).selectOption(savings.id);
    await page.getByLabel('Amount (MYR)', { exact: true }).fill('100');
    await page.getByRole('button', { name: 'Confirm transaction', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.goto('/#/finance/budgets');
    await page.getByRole('button', { name: 'New budget', exact: true }).click();
    await page.getByLabel('Monthly limit (MYR)', { exact: true }).fill('100');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByText('MYR 75.00', { exact: true })).toBeVisible();
    await page.goto('/#/finance/bills');
    await page.getByRole('button', { name: 'New bill', exact: true }).click();
    await page.getByLabel('Bill name', { exact: true }).fill('Internet');
    await page.getByRole('combobox', { name: 'Paying account', exact: true }).selectOption(bank.id);
    await page.getByLabel('Expected amount (MYR)', { exact: true }).fill('50');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Pay Internet', exact: true }).first().click();
    await page.getByRole('button', { name: 'Record payment', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    const [bill] = await bridge.call<Bill[]>('list_bills', { workspaceId: workspace.id });
    const filter = {
      workspaceId: workspace.id,
      from: null,
      to: null,
      accountId: null,
      categoryId: null,
      kind: null,
      search: null,
      minAmountMinor: null,
      maxAmountMinor: null,
      limit: 50,
      offset: 0,
    };
    let txs = await bridge.call<TransactionPage>('list_transactions', { filter });
    expect(txs.items.filter((t) => t.billId === bill.id)).toHaveLength(1);
    const paid = txs.items.find((t) => t.billId === bill.id)!;
    await bridge.call('pay_bill', {
      input: {
        billId: bill.id,
        dueOn: paid.billDueOn,
        paidOn: paid.occurredOn,
        amountMinor: 5000,
        accountId: bank.id,
        paymentMethod: null,
        note: '',
      },
    });
    txs = await bridge.call<TransactionPage>('list_transactions', { filter });
    expect(txs.items.filter((t) => t.billId === bill.id)).toHaveLength(1);
    await page.goto('/#/finance/reports');
    await expect(page.getByText('MYR 200.00', { exact: true }).first()).toBeVisible();
    await expect(page.getByText('MYR 75.00', { exact: true }).first()).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath('finance-report.png'), fullPage: true });
    await page.goto('about:blank');
    await bridge.close();
    bridge = new RustBridge(path.join(directory, 'app'));
    await page.goto('/#/finance/accounts');
    await expect(page.getByText('MYR 1,025.00', { exact: true })).toBeVisible();
    await expect(page.getByText('MYR 100.00', { exact: true }).first()).toBeVisible();
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});

test('business workspace invoice payment counts income once and moves stock on issue', async ({
  page,
}, testInfo) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'folio-business-ui-'));
  const bridge = new RustBridge(path.join(directory, 'app'));
  await installBridge(page, bridge, []);
  await page.goto('about:blank');
  try {
    await page.goto('/#/finance/accounts');
    await page.getByRole('button', { name: 'New business workspace', exact: true }).click();
    await page.getByLabel('Workspace name', { exact: true }).fill('Studio books');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    const workspace = (await bridge.call<Workspace[]>('list_workspaces')).find(
      (w) => w.name === 'Studio books',
    )!;
    await page.getByRole('combobox', { name: 'Finance workspace', exact: true }).selectOption(workspace.id);
    await page.getByRole('button', { name: 'New account', exact: true }).click();
    await page.getByLabel('Account name', { exact: true }).fill('Business bank');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('link', { name: 'Business', exact: true }).click();
    await page.getByRole('button', { name: 'Business profile', exact: true }).click();
    await page.getByLabel('Legal business name', { exact: true }).fill('Studio Sdn Bhd');
    await page.getByLabel('Business address', { exact: true }).fill('Kuala Lumpur');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Customers', exact: true }).click();
    await page.getByRole('button', { name: 'New customer', exact: true }).click();
    await page.getByLabel('Customer name', { exact: true }).fill('Client books');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Inventory', exact: true }).click();
    await page.getByRole('button', { name: 'New inventory item', exact: true }).click();
    await page.getByLabel('SKU', { exact: true }).fill('BOOK');
    await page.getByLabel('Item name', { exact: true }).fill('Notebook');
    await page.getByLabel('Unit price', { exact: true }).fill('20');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Adjust stock', exact: true }).click();
    await page.getByLabel('Quantity change', { exact: true }).fill('10');
    await page.getByLabel('Adjustment reason', { exact: true }).fill('Opening stock');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Invoices', exact: true }).click();
    await page.getByRole('button', { name: 'New invoice', exact: true }).click();
    const [item] = await bridge.call<import('../../src/bindings/generated').InventoryItem[]>(
      'list_inventory_items',
      { workspaceId: workspace.id },
    );
    await page.getByRole('combobox', { name: 'Inventory item 1', exact: true }).selectOption(item.id);
    await page.getByRole('button', { name: 'Save draft invoice', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Open invoice', exact: true }).click();
    await page.getByRole('button', { name: 'Issue invoice', exact: true }).click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: 'Issue invoice', exact: true })
      .click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page.getByRole('button', { name: 'Record payment received', exact: true }).click();
    await page.getByRole('button', { name: 'Record invoice payment', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    const [invoice] = await bridge.call<import('../../src/bindings/generated').Invoice[]>(
      'list_invoices',
      { workspaceId: workspace.id },
    );
    expect(invoice.status).toBe('paid');
    expect(invoice.paidMinor).toBe(2000);
    const [updatedItem] = await bridge.call<import('../../src/bindings/generated').InventoryItem[]>(
      'list_inventory_items',
      { workspaceId: workspace.id },
    );
    expect(updatedItem.stockMilli).toBe(9000);
    const filter = {
      workspaceId: workspace.id,
      from: null,
      to: null,
      accountId: null,
      categoryId: null,
      kind: 'income',
      search: null,
      minAmountMinor: null,
      maxAmountMinor: null,
      limit: 50,
      offset: 0,
    };
    const txs = await bridge.call<TransactionPage>('list_transactions', { filter });
    expect(txs.items).toHaveLength(1);
    expect(txs.items[0].amountMinor).toBe(2000);
    await page.screenshot({ path: testInfo.outputPath('paid-invoice.png'), fullPage: true });
    await page.getByRole('button', { name: 'Export invoice PDF', exact: true }).click();
    await expect(page.locator('.inline-error')).toHaveCount(0);
  } finally {
    await page.goto('about:blank');
    await bridge.close();
  }
});
