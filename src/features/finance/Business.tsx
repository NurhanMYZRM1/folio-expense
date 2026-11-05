import { useCallback, useState } from 'react';
import { Link } from 'react-router-dom';
import type {
  BusinessProfile,
  Customer,
  InventoryItem,
  Invoice,
  InvoiceInput,
  InvoiceLineInput,
} from '../../bindings/generated';
import { api, localToday } from '../../lib/ipc';
import { CURRENCIES, formatMoney, moneyInput, parseMoney } from '../../lib/money';
import { errorMessage } from '../../lib/errors';
import { exportInvoicePdf } from '../../lib/invoicePdf';
import { EmptyState, Loading, Panel } from '../../components/ui';
import {
  AccountOptions,
  Field,
  FinanceDialog,
  FinanceFrame,
  QueryError,
  parseSigned,
  requiredMoney,
  signedInput,
  useFinance,
  useQuery,
} from './shared';

function ProfileEditor({ profile, close }: { profile: BusinessProfile | null; close: () => void }) {
  const { workspace, refreshFinance } = useFinance();
  const [form, setForm] = useState<BusinessProfile>(
    profile ?? {
      workspaceId: workspace!.id,
      legalName: '',
      registrationNumber: '',
      tin: '',
      sstRegistrationNumber: '',
      address: '',
      email: '',
      phone: '',
      invoicePrefix: 'INV',
      version: 0,
    },
  );
  return (
    <FinanceDialog
      title="Business profile"
      close={close}
      save={async () => {
        await api.saveBusinessProfile(form);
        await refreshFinance();
      }}
    >
      {(
        [
          ['legalName', 'Legal business name'],
          ['registrationNumber', 'Business registration number'],
          ['tin', 'Business TIN'],
          ['sstRegistrationNumber', 'SST registration number'],
          ['address', 'Business address'],
          ['email', 'Business email'],
          ['phone', 'Business phone'],
          ['invoicePrefix', 'Invoice number prefix'],
        ] as const
      ).map(([key, label]) => (
        <Field label={label} key={key}>
          <input
            required={key === 'legalName' || key === 'address' || key === 'invoicePrefix'}
            maxLength={key === 'address' ? 1000 : key === 'invoicePrefix' ? 12 : 200}
            value={form[key]}
            onChange={(e) => setForm((v) => ({ ...v, [key]: e.target.value }))}
          />
        </Field>
      ))}
      <p className="muted">
        Issued invoices keep a snapshot of these details. Only provide an SST registration number if
        registered.
      </p>
    </FinanceDialog>
  );
}
function CustomerEditor({ customer, close }: { customer?: Customer; close: () => void }) {
  const { workspace, refreshFinance } = useFinance();
  const [form, setForm] = useState({
    name: customer?.name ?? '',
    registrationNumber: customer?.registrationNumber ?? '',
    tin: customer?.tin ?? '',
    address: customer?.address ?? '',
    email: customer?.email ?? '',
    phone: customer?.phone ?? '',
    archived: customer?.archived ?? false,
  });
  return (
    <FinanceDialog
      title={customer ? 'Edit customer' : 'New customer'}
      close={close}
      save={async () => {
        await api.saveCustomer({
          ...form,
          id: customer?.id ?? null,
          version: customer?.version ?? null,
          workspaceId: workspace!.id,
        });
        await refreshFinance();
      }}
    >
      {(
        [
          ['name', 'Customer name'],
          ['registrationNumber', 'Customer registration number'],
          ['tin', 'Customer TIN'],
          ['address', 'Customer address'],
          ['email', 'Customer email'],
          ['phone', 'Customer phone'],
        ] as const
      ).map(([key, label]) => (
        <Field key={key} label={label}>
          <input
            required={key === 'name'}
            maxLength={key === 'address' ? 1000 : 200}
            value={form[key]}
            onChange={(e) => setForm((v) => ({ ...v, [key]: e.target.value }))}
          />
        </Field>
      ))}
      {customer && (
        <label className="check-field">
          <input
            type="checkbox"
            checked={form.archived}
            onChange={(e) => setForm((v) => ({ ...v, archived: e.target.checked }))}
          />{' '}
          Archive customer
        </label>
      )}
    </FinanceDialog>
  );
}
function ItemEditor({ item, close }: { item?: InventoryItem; close: () => void }) {
  const { workspace, refreshFinance } = useFinance();
  const [form, setForm] = useState({
    sku: item?.sku ?? '',
    name: item?.name ?? '',
    unit: item?.unit ?? 'unit',
    currency: item?.currency ?? workspace!.baseCurrency,
    price: item ? moneyInput(item.unitPriceMinor, item.currency) : '',
    archived: item?.archived ?? false,
  });
  return (
    <FinanceDialog
      title={item ? 'Edit inventory item' : 'New inventory item'}
      close={close}
      save={async () => {
        await api.saveInventoryItem({
          id: item?.id ?? null,
          version: item?.version ?? null,
          workspaceId: workspace!.id,
          sku: form.sku,
          name: form.name,
          unit: form.unit,
          currency: form.currency,
          unitPriceMinor: parseMoney(form.price, form.currency) ?? 0,
          archived: form.archived,
        });
        await refreshFinance();
      }}
    >
      <Field label="SKU">
        <input
          required
          maxLength={80}
          value={form.sku}
          onChange={(e) => setForm((v) => ({ ...v, sku: e.target.value }))}
        />
      </Field>
      <Field label="Item name">
        <input
          required
          maxLength={200}
          value={form.name}
          onChange={(e) => setForm((v) => ({ ...v, name: e.target.value }))}
        />
      </Field>
      <Field label="Unit label">
        <input
          required
          maxLength={40}
          value={form.unit}
          onChange={(e) => setForm((v) => ({ ...v, unit: e.target.value }))}
        />
      </Field>
      <div className="form-grid">
        <Field label="Price currency">
          <select
            value={form.currency}
            onChange={(e) => setForm((v) => ({ ...v, currency: e.target.value }))}
          >
            {CURRENCIES.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
        </Field>
        <Field label="Unit price">
          <input
            required
            inputMode="decimal"
            value={form.price}
            onChange={(e) => setForm((v) => ({ ...v, price: e.target.value }))}
          />
        </Field>
      </div>
      {item && (
        <label className="check-field">
          <input
            type="checkbox"
            checked={form.archived}
            onChange={(e) => setForm((v) => ({ ...v, archived: e.target.checked }))}
          />{' '}
          Archive item
        </label>
      )}
      <p className="muted">
        Use stock adjustments for opening stock, purchases or corrections. Invoices remove stock
        when issued and restore it when voided.
      </p>
    </FinanceDialog>
  );
}
function StockEditor({ item, close }: { item: InventoryItem; close: () => void }) {
  const { workspace, refreshFinance } = useFinance();
  const [requestId] = useState(() => crypto.randomUUID()),
    [quantity, setQuantity] = useState(''),
    [reason, setReason] = useState(''),
    [date, setDate] = useState(localToday());
  return (
    <FinanceDialog
      title={`Adjust stock: ${item.name}`}
      close={close}
      save={async () => {
        await api.adjustStock({
          requestId,
          workspaceId: workspace!.id,
          itemId: item.id,
          quantityMilli: parseSigned(quantity, 'KWD'),
          occurredOn: date,
          reason,
        });
        await refreshFinance();
      }}
    >
      <Field label="Quantity change">
        <input
          required
          inputMode="decimal"
          value={quantity}
          placeholder="10 or -2.500"
          onChange={(e) => setQuantity(e.target.value)}
        />
      </Field>
      <p className="muted">
        Positive adds stock; negative removes stock. Up to three decimal places.
      </p>
      <Field label="Stock adjustment date">
        <input required type="date" value={date} onChange={(e) => setDate(e.target.value)} />
      </Field>
      <Field label="Adjustment reason">
        <input
          required
          maxLength={1000}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </Field>
    </FinanceDialog>
  );
}
function StockHistory({ item, close }: { item: InventoryItem; close: () => void }) {
  const { workspace } = useFinance();
  const query = useQuery(
    useCallback(() => api.stockMovements(workspace!.id, item.id), [workspace, item.id]),
  );
  return (
    <FinanceDialog
      title={`Stock movements: ${item.name}`}
      close={close}
      save={async () => {}}
      label="Done"
    >
      <QueryError error={query.error} />
      {query.loading ? (
        <Loading />
      ) : query.data?.length ? (
        query.data.map((m) => (
          <div key={m.id} className="finance-list-row">
            <div>
              <strong>
                {signedInput(m.quantityMilli, 'KWD')} {item.unit}
              </strong>
              <small>
                {m.occurredOn} · {m.reason}
              </small>
            </div>
          </div>
        ))
      ) : (
        <p>No stock movements yet.</p>
      )}
    </FinanceDialog>
  );
}
type LineForm = {
  inventoryItemId: string;
  description: string;
  quantity: string;
  price: string;
  discount: string;
  tax: string;
  classification: string;
};
function toLineForm(line: InvoiceLineInput, currency: string): LineForm {
  return {
    inventoryItemId: line.inventoryItemId ?? '',
    description: line.description,
    quantity: moneyInput(line.quantityMilli, 'KWD'),
    price: moneyInput(line.unitPriceMinor, currency),
    discount: moneyInput(line.discountMinor, currency),
    tax: moneyInput(line.taxBasisPoints, 'MYR'),
    classification: line.classificationCode,
  };
}
function InvoiceEditor({
  invoice,
  customers,
  items,
  close,
}: {
  invoice?: Invoice;
  customers: Customer[];
  items: InventoryItem[];
  close: () => void;
}) {
  const { workspace, refreshFinance } = useFinance();
  const [customer, setCustomer] = useState(
      invoice?.customerId ?? customers.find((c) => !c.archived)?.id ?? '',
    ),
    [currency, setCurrency] = useState(invoice?.currency ?? workspace!.baseCurrency),
    [issued, setIssued] = useState(invoice?.issuedOn ?? localToday()),
    [due, setDue] = useState(invoice?.dueOn ?? localToday()),
    [note, setNote] = useState(invoice?.note ?? '');
  const blank = (): LineForm => ({
    inventoryItemId: '',
    description: '',
    quantity: '1',
    price: '',
    discount: '0',
    tax: '0',
    classification: '',
  });
  const [lines, setLines] = useState<LineForm[]>(
    invoice?.lines.map((l) => toLineForm(l, invoice.currency)) ?? [blank()],
  );
  function update(index: number, key: keyof LineForm, value: string) {
    setLines((v) => v.map((line, i) => (i === index ? { ...line, [key]: value } : line)));
  }
  return (
    <FinanceDialog
      title={invoice ? 'Edit draft invoice' : 'New draft invoice'}
      close={close}
      label="Save draft invoice"
      save={async () => {
        const input: InvoiceInput = {
          id: invoice?.id ?? null,
          version: invoice?.version ?? null,
          workspaceId: workspace!.id,
          customerId: customer,
          currency,
          issuedOn: issued,
          dueOn: due,
          note,
          lines: lines.map((l) => ({
            inventoryItemId: l.inventoryItemId || null,
            description: l.description,
            quantityMilli: requiredMoney(l.quantity, 'KWD'),
            unitPriceMinor: parseMoney(l.price, currency) ?? 0,
            discountMinor: parseMoney(l.discount, currency) ?? 0,
            taxBasisPoints: parseMoney(l.tax, 'MYR') ?? 0,
            classificationCode: l.classification,
          })),
        };
        await api.saveInvoice(input);
        await refreshFinance();
      }}
    >
      <Field label="Invoice customer">
        <select required value={customer} onChange={(e) => setCustomer(e.target.value)}>
          <option value="">Choose customer</option>
          {customers
            .filter((c) => !c.archived || c.id === customer)
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
        </select>
      </Field>
      <div className="form-grid">
        <Field label="Invoice currency">
          <select value={currency} onChange={(e) => setCurrency(e.target.value)}>
            {CURRENCIES.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
        </Field>
        <Field label="Invoice date">
          <input type="date" required value={issued} onChange={(e) => setIssued(e.target.value)} />
        </Field>
      </div>
      <Field label="Invoice due date">
        <input type="date" required value={due} onChange={(e) => setDue(e.target.value)} />
      </Field>
      {lines.map((line, index) => (
        <section className="finance-invoice-line" key={index}>
          <h3>Line {index + 1}</h3>
          <Field label={`Inventory item ${index + 1}`}>
            <select
              value={line.inventoryItemId}
              onChange={(e) => {
                const item = items.find((i) => i.id === e.target.value);
                setLines((v) =>
                  v.map((l, n) =>
                    n === index
                      ? {
                          ...l,
                          inventoryItemId: e.target.value,
                          ...(item
                            ? {
                                description: item.name,
                                price: moneyInput(item.unitPriceMinor, currency),
                              }
                            : {}),
                        }
                      : l,
                  ),
                );
              }}
            >
              <option value="">Service / custom line</option>
              {items
                .filter((i) => !i.archived && i.currency === currency)
                .map((i) => (
                  <option key={i.id} value={i.id}>
                    {i.sku} · {i.name}
                  </option>
                ))}
            </select>
          </Field>
          <Field label={`Line ${index + 1} description`}>
            <input
              required
              maxLength={500}
              value={line.description}
              onChange={(e) => update(index, 'description', e.target.value)}
            />
          </Field>
          <div className="form-grid">
            <Field label={`Line ${index + 1} quantity`}>
              <input
                inputMode="decimal"
                required
                value={line.quantity}
                onChange={(e) => update(index, 'quantity', e.target.value)}
              />
            </Field>
            <Field label={`Line ${index + 1} unit price`}>
              <input
                inputMode="decimal"
                required
                value={line.price}
                onChange={(e) => update(index, 'price', e.target.value)}
              />
            </Field>
          </div>
          <div className="form-grid">
            <Field label={`Line ${index + 1} discount`}>
              <input
                inputMode="decimal"
                value={line.discount}
                onChange={(e) => update(index, 'discount', e.target.value)}
              />
            </Field>
            <Field label={`Line ${index + 1} tax percent`}>
              <input
                inputMode="decimal"
                value={line.tax}
                onChange={(e) => update(index, 'tax', e.target.value)}
              />
            </Field>
          </div>
          <Field label={`Line ${index + 1} MyInvois classification`}>
            <input
              maxLength={3}
              value={line.classification}
              onChange={(e) => update(index, 'classification', e.target.value)}
            />
          </Field>
          {lines.length > 1 && (
            <button
              type="button"
              className="text-button danger-text"
              onClick={() => setLines((v) => v.filter((_, i) => i !== index))}
            >
              Remove line {index + 1}
            </button>
          )}
        </section>
      ))}
      <button
        className="button secondary"
        type="button"
        disabled={lines.length >= 100}
        onClick={() => setLines((v) => [...v, blank()])}
      >
        Add invoice line
      </button>
      <p className="muted">
        Tax rates are your selection. Verify SST registration and applicability against Customs
        guidance. Saved totals are calculated in the core; a draft does not move stock or record
        income.
      </p>
      <Field label="Invoice notes">
        <textarea
          rows={2}
          maxLength={4000}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </Field>
    </FinanceDialog>
  );
}
function InvoicePayment({ invoice, close }: { invoice: Invoice; close: () => void }) {
  const { workspace, accounts, refreshFinance } = useFinance();
  const [requestId] = useState(() => crypto.randomUUID()),
    [amount, setAmount] = useState(
      moneyInput(invoice.totalMinor - invoice.paidMinor, invoice.currency),
    ),
    [account, setAccount] = useState(
      accounts.find((a) => a.currency === invoice.currency && !a.archivedAt)?.id ?? '',
    ),
    [date, setDate] = useState(localToday()),
    [note, setNote] = useState('');
  return (
    <FinanceDialog
      title={`Payment for ${invoice.number}`}
      close={close}
      label="Record invoice payment"
      save={async () => {
        await api.recordInvoicePayment({
          requestId,
          workspaceId: workspace!.id,
          invoiceId: invoice.id,
          accountId: account,
          amountMinor: requiredMoney(amount, invoice.currency),
          paidOn: date,
          note,
        });
        await refreshFinance();
      }}
    >
      <Field label="Receiving account">
        <select required value={account} onChange={(e) => setAccount(e.target.value)}>
          <option value="">Choose matching currency account</option>
          <AccountOptions accounts={accounts.filter((a) => a.currency === invoice.currency)} />
        </select>
      </Field>
      <Field label={`Payment received (${invoice.currency})`}>
        <input
          required
          inputMode="decimal"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
      </Field>
      <Field label="Payment received on">
        <input required type="date" value={date} onChange={(e) => setDate(e.target.value)} />
      </Field>
      <Field label="Payment reference / notes">
        <input maxLength={4000} value={note} onChange={(e) => setNote(e.target.value)} />
      </Field>
      <p className="muted">
        One payment records one income transaction. Issuing the invoice itself does not record
        income.
      </p>
    </FinanceDialog>
  );
}
function BusinessContent() {
  const { workspace, refreshFinance } = useFinance();
  const [tab, setTab] = useState('Invoices'),
    [profileEdit, setProfileEdit] = useState(false),
    [customerEdit, setCustomerEdit] = useState<Customer | 'new' | null>(null),
    [itemEdit, setItemEdit] = useState<InventoryItem | 'new' | null>(null),
    [stock, setStock] = useState<InventoryItem | null>(null),
    [history, setHistory] = useState<InventoryItem | null>(null),
    [invoiceEdit, setInvoiceEdit] = useState<Invoice | 'new' | null>(null),
    [selected, setSelected] = useState(''),
    [payment, setPayment] = useState<Invoice | null>(null),
    [confirm, setConfirm] = useState<{ invoice: Invoice; action: 'issue' | 'void' } | null>(null),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false),
    [validation, setValidation] = useState<{
      errors: string[];
      warnings: string[];
      preparationStatus: string;
      sourceUrl: string;
    } | null>(null);
  const query = useQuery(
    useCallback(async () => {
      const [profile, customers, items, invoices, report] = await Promise.all([
        api.businessProfile(workspace!.id),
        api.customers(workspace!.id, true),
        api.inventoryItems(workspace!.id, true),
        api.invoices(workspace!.id),
        api.businessReport(workspace!.id, localToday()),
      ]);
      return { profile, customers, items, invoices, report };
    }, [workspace]),
  );
  async function action(run: () => Promise<unknown>) {
    setBusy(true);
    setError('');
    try {
      await run();
      await refreshFinance();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const data = query.data,
    invoice = data?.invoices.find((i) => i.id === selected);
  if (query.loading) return <Loading />;
  return (
    <>
      <QueryError error={query.error || error} />
      {data && (
        <>
          <div className="finance-business-actions">
            {['Invoices', 'Customers', 'Inventory', 'Business report'].map((t) => (
              <button
                key={t}
                className={`button ${tab === t ? 'primary' : 'secondary'}`}
                onClick={() => {
                  setTab(t);
                  setSelected('');
                  setValidation(null);
                }}
              >
                {t}
              </button>
            ))}
            <button className="text-button" onClick={() => setProfileEdit(true)}>
              Business profile
            </button>
          </div>
          {!data.profile && (
            <p className="notice-box">Complete your business profile before issuing an invoice.</p>
          )}
          {tab === 'Customers' && (
            <Panel
              title="Customers"
              action={
                <button className="text-button" onClick={() => setCustomerEdit('new')}>
                  New customer
                </button>
              }
            >
              {data.customers.length ? (
                data.customers.map((c) => (
                  <div key={c.id} className="finance-list-row">
                    <div>
                      <strong>{c.name}</strong>
                      <small>
                        {c.email || c.address || 'No contact details'}
                        {c.archived ? ' · archived' : ''}
                      </small>
                    </div>
                    <button className="text-button" onClick={() => setCustomerEdit(c)}>
                      Edit {c.name}
                    </button>
                  </div>
                ))
              ) : (
                <EmptyState
                  title="Add your first customer"
                  description="Customer details are copied to an invoice when it is issued."
                />
              )}
            </Panel>
          )}
          {tab === 'Inventory' && (
            <Panel
              title="Inventory"
              action={
                <button className="text-button" onClick={() => setItemEdit('new')}>
                  New inventory item
                </button>
              }
            >
              {data.items.length ? (
                data.items.map((i) => (
                  <div key={i.id} className="finance-list-row">
                    <div>
                      <strong>{i.name}</strong>
                      <small>
                        {i.sku} · {formatMoney(i.unitPriceMinor, i.currency)} / {i.unit}
                        {i.archived ? ' · archived' : ''}
                      </small>
                    </div>
                    <strong>
                      {moneyInput(i.stockMilli, 'KWD')} {i.unit}
                    </strong>
                    <button className="text-button" onClick={() => setStock(i)}>
                      Adjust stock
                    </button>
                    <button className="text-button" onClick={() => setHistory(i)}>
                      Movements
                    </button>
                    <button className="text-button" onClick={() => setItemEdit(i)}>
                      Edit {i.name}
                    </button>
                  </div>
                ))
              ) : (
                <EmptyState
                  title="Track your stock"
                  description="Create an inventory item, then add opening stock with an adjustment."
                />
              )}
            </Panel>
          )}
          {tab === 'Business report' && (
            <Panel title={`Business report · ${data.report.asOf}`}>
              <p className="finance-panel-copy">
                {data.report.draftCount} draft invoices · {data.report.issuedCount} issued invoices.
                Each currency is shown separately. Invoice totals and receipts are distinct:
                payments are the income events in finance reports.
              </p>
              {data.report.byCurrency.length ? (
                <div className="table-wrap">
                  <table className="finance-table">
                    <thead>
                      <tr>
                        <th>Currency</th>
                        <th>Invoiced</th>
                        <th>Paid</th>
                        <th>Outstanding</th>
                        <th>Overdue</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.report.byCurrency.map((t) => (
                        <tr key={t.currency}>
                          <td>{t.currency}</td>
                          <td>{formatMoney(t.invoicedMinor, t.currency)}</td>
                          <td>{formatMoney(t.paidMinor, t.currency)}</td>
                          <td>{formatMoney(t.outstandingMinor, t.currency)}</td>
                          <td>{formatMoney(t.overdueMinor, t.currency)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <p className="finance-panel-copy">Issue an invoice to see its totals here.</p>
              )}
            </Panel>
          )}
          {tab === 'Invoices' &&
            (invoice ? (
              <>
                <Panel
                  title={`Invoice ${invoice.number ?? 'draft'}`}
                  action={
                    <button
                      className="text-button"
                      onClick={() => {
                        setSelected('');
                        setValidation(null);
                      }}
                    >
                      All invoices
                    </button>
                  }
                >
                  <div className="finance-invoice-summary">
                    <div>
                      <strong>{invoice.customer.name}</strong>
                      <p>{invoice.customer.address}</p>
                      <p>
                        Issued {invoice.issuedOn} · Due {invoice.dueOn}
                      </p>
                    </div>
                    <span className="badge">{invoice.status}</span>
                  </div>
                  <div className="table-wrap">
                    <table className="finance-table">
                      <thead>
                        <tr>
                          <th>Description</th>
                          <th>Quantity</th>
                          <th>Unit price</th>
                          <th>Discount</th>
                          <th>Tax</th>
                          <th>Total</th>
                        </tr>
                      </thead>
                      <tbody>
                        {invoice.lines.map((line, index) => (
                          <tr key={index}>
                            <td>{line.description}</td>
                            <td>{moneyInput(line.quantityMilli, 'KWD')}</td>
                            <td>{formatMoney(line.unitPriceMinor, invoice.currency)}</td>
                            <td>{formatMoney(line.discountMinor, invoice.currency)}</td>
                            <td>{formatMoney(line.taxMinor, invoice.currency)}</td>
                            <td>{formatMoney(line.totalMinor, invoice.currency)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <div className="finance-invoice-totals">
                    <p>
                      Subtotal{' '}
                      <strong>{formatMoney(invoice.subtotalMinor, invoice.currency)}</strong>
                    </p>
                    <p>
                      Tax <strong>{formatMoney(invoice.taxMinor, invoice.currency)}</strong>
                    </p>
                    <p>
                      Total <strong>{formatMoney(invoice.totalMinor, invoice.currency)}</strong>
                    </p>
                    <p>
                      Paid <strong>{formatMoney(invoice.paidMinor, invoice.currency)}</strong>
                    </p>
                    <p>
                      Outstanding{' '}
                      <strong>
                        {formatMoney(
                          Math.max(0, invoice.totalMinor - invoice.paidMinor),
                          invoice.currency,
                        )}
                      </strong>
                    </p>
                  </div>
                  {invoice.note && <p className="finance-panel-copy">{invoice.note}</p>}
                  <div className="finance-row-actions">
                    <button
                      className="button secondary"
                      disabled={busy}
                      onClick={() => void action(() => exportInvoicePdf(invoice))}
                    >
                      Export invoice PDF
                    </button>
                    {invoice.status === 'draft' && (
                      <>
                        <button
                          className="button secondary"
                          onClick={() => setInvoiceEdit(invoice)}
                        >
                          Edit draft
                        </button>
                        <button
                          className="button primary"
                          onClick={() => setConfirm({ invoice, action: 'issue' })}
                        >
                          Issue invoice
                        </button>
                      </>
                    )}
                    {invoice.status === 'issued' && (
                      <button className="button primary" onClick={() => setPayment(invoice)}>
                        Record payment received
                      </button>
                    )}
                    {invoice.status !== 'void' && (
                      <button
                        className="button secondary"
                        disabled={busy}
                        onClick={() =>
                          void action(async () => {
                            setValidation(await api.validateEinvoice(workspace!.id, invoice.id));
                          })
                        }
                      >
                        Check e-invoice preparation
                      </button>
                    )}
                    {(invoice.status === 'issued' || invoice.status === 'draft') && (
                      <button
                        className="text-button danger-text"
                        onClick={() => setConfirm({ invoice, action: 'void' })}
                      >
                        Void invoice
                      </button>
                    )}
                  </div>
                </Panel>
                {validation && (
                  <Panel title={`Local e-invoice preparation: ${validation.preparationStatus}`}>
                    <div className="finance-panel-copy">
                      <p>Nothing has been submitted to MyInvois.</p>
                      {validation.errors.map((e, i) => (
                        <p key={`e${i}`} className="danger-text">
                          {e}
                        </p>
                      ))}
                      {validation.warnings.map((w, i) => (
                        <p key={`w${i}`}>{w}</p>
                      ))}
                      <a href={validation.sourceUrl} target="_blank" rel="noreferrer">
                        Official MyInvois documentation
                      </a>
                    </div>
                  </Panel>
                )}
                <Panel title="Payment history">
                  {invoice.payments.length ? (
                    invoice.payments.map((p) => (
                      <div className="finance-list-row" key={p.id}>
                        <div>
                          <strong>{formatMoney(p.amountMinor, invoice.currency)}</strong>
                          <small>
                            {p.paidOn} · {p.note}
                          </small>
                        </div>
                        <Link className="text-link" to="/finance/transactions">
                          View ledger
                        </Link>
                      </div>
                    ))
                  ) : (
                    <p className="finance-panel-copy">
                      No payments recorded. Issuing an invoice does not count it as received income.
                    </p>
                  )}
                </Panel>
              </>
            ) : (
              <Panel
                title="Invoices"
                action={
                  <button
                    className="text-button"
                    disabled={!data.customers.some((c) => !c.archived)}
                    onClick={() => setInvoiceEdit('new')}
                  >
                    New invoice
                  </button>
                }
              >
                {data.invoices.length ? (
                  data.invoices.map((i) => (
                    <div className="finance-list-row" key={i.id}>
                      <div>
                        <strong>
                          {i.number ?? 'Draft invoice'} · {i.customer.name}
                        </strong>
                        <small>
                          Due {i.dueOn} · {i.status}
                        </small>
                      </div>
                      <strong>{formatMoney(i.totalMinor, i.currency)}</strong>
                      <button
                        className="button secondary"
                        onClick={() => {
                          setSelected(i.id);
                          setValidation(null);
                        }}
                      >
                        Open invoice
                      </button>
                    </div>
                  ))
                ) : (
                  <EmptyState
                    title="Your next invoice starts here"
                    description={
                      data.customers.length
                        ? 'Create a draft, review its totals and issue it when ready.'
                        : 'Add a customer before creating an invoice.'
                    }
                  />
                )}
              </Panel>
            ))}
          {profileEdit && (
            <ProfileEditor profile={data.profile} close={() => setProfileEdit(false)} />
          )}
          {customerEdit && (
            <CustomerEditor
              customer={customerEdit === 'new' ? undefined : customerEdit}
              close={() => setCustomerEdit(null)}
            />
          )}
          {itemEdit && (
            <ItemEditor
              item={itemEdit === 'new' ? undefined : itemEdit}
              close={() => setItemEdit(null)}
            />
          )}
          {stock && <StockEditor item={stock} close={() => setStock(null)} />}
          {history && <StockHistory item={history} close={() => setHistory(null)} />}
          {invoiceEdit && (
            <InvoiceEditor
              invoice={invoiceEdit === 'new' ? undefined : invoiceEdit}
              customers={data.customers}
              items={data.items}
              close={() => setInvoiceEdit(null)}
            />
          )}
          {payment && <InvoicePayment invoice={payment} close={() => setPayment(null)} />}
          {confirm && (
            <FinanceDialog
              title={confirm.action === 'issue' ? 'Issue this invoice?' : 'Void this invoice?'}
              close={() => setConfirm(null)}
              label={confirm.action === 'issue' ? 'Issue invoice' : 'Void invoice'}
              save={async () => {
                if (confirm.action === 'issue')
                  await api.issueInvoice(
                    workspace!.id,
                    confirm.invoice.id,
                    confirm.invoice.version,
                  );
                else
                  await api.voidInvoice(workspace!.id, confirm.invoice.id, confirm.invoice.version);
                await refreshFinance();
              }}
            >
              <p>
                {confirm.action === 'issue'
                  ? 'Issuing assigns a permanent invoice number and removes inventory stock. Invoice details are then fixed. Income is recorded only when a payment is received.'
                  : 'Voiding restores stock moved by an issued invoice. Payment history is protected; a paid invoice cannot be voided.'}
              </p>
            </FinanceDialog>
          )}
        </>
      )}
    </>
  );
}
export function Business() {
  const { workspace } = useFinance();
  return (
    <FinanceFrame
      title="Business tools"
      subtitle="Customers, invoices and inventory, separate from personal finances."
    >
      {workspace?.kind === 'business' ? (
        <BusinessContent key={workspace.id} />
      ) : (
        <EmptyState
          title="Choose a business workspace"
          description="Create a business workspace from Accounts & goals, then select it above to manage customers, stock and invoices."
          action={
            <Link to="/finance/accounts" className="button primary">
              Manage workspaces
            </Link>
          }
        />
      )}
    </FinanceFrame>
  );
}
