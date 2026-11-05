import { useCallback, useState } from 'react';
import type { ReceiptItemsInput, ReceiptItemsReview } from '../../bindings/generated';
import { Loading, Panel } from '../../components/ui';
import { api } from '../../lib/ipc';
import { errorMessage } from '../../lib/errors';
import { CURRENCIES, moneyInput, parseMoney } from '../../lib/money';
import { Field, QueryError, signedMoney, useQuery } from '../finance/shared';

type EditableLine = {
  description: string;
  quantity: string;
  unitPrice: string;
  amount: string;
  confidence: number;
};
function decimal(value: string, precisionCurrency: string) {
  return parseMoney(value, precisionCurrency) ?? 0;
}
function ReviewEditor({ initial, locked }: { initial: ReceiptItemsReview; locked: boolean }) {
  const [currency, setCurrency] = useState(initial.currency);
  const [items, setItems] = useState<EditableLine[]>(() =>
    initial.items.map((item) => ({
      description: item.description,
      quantity: item.quantityMilli === null ? '' : moneyInput(item.quantityMilli, 'KWD'),
      unitPrice: moneyInput(item.unitPriceMinor, currency),
      amount: moneyInput(item.amountMinor, currency),
      confidence: item.confidence,
    })),
  );
  const [tax, setTax] = useState(moneyInput(initial.adjustments.taxMinor, currency));
  const [discount, setDiscount] = useState(moneyInput(initial.adjustments.discountMinor, currency));
  const [service, setService] = useState(
    moneyInput(initial.adjustments.serviceChargeMinor, currency),
  );
  const [rounding, setRounding] = useState(
    `${initial.adjustments.roundingMinor < 0 ? '-' : ''}${moneyInput(Math.abs(initial.adjustments.roundingMinor), currency)}`,
  );
  const [included, setIncluded] = useState(initial.adjustments.taxIncluded);
  const [saved, setSaved] = useState(initial);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  function line(index: number, change: Partial<EditableLine>) {
    setItems((previous) =>
      previous.map((value, row) =>
        row === index ? { ...value, ...change, confidence: 1 } : value,
      ),
    );
    setDirty(true);
  }
  function input(reviewed: boolean): ReceiptItemsInput {
    const sign = rounding.trim().startsWith('-') ? -1 : 1;
    return {
      expenseId: initial.expenseId,
      expenseVersion: saved.expenseVersion,
      version: saved.version,
      currency,
      items: items.map((item) => ({
        description: item.description.trim(),
        quantityMilli: item.quantity.trim() ? decimal(item.quantity, 'KWD') : null,
        unitPriceMinor: item.unitPrice.trim() ? decimal(item.unitPrice, currency) : null,
        amountMinor: decimal(item.amount, currency),
        confidence: item.confidence,
      })),
      adjustments: {
        taxIncluded: included,
        taxMinor: decimal(tax, currency),
        discountMinor: decimal(discount, currency),
        serviceChargeMinor: decimal(service, currency),
        roundingMinor: sign * decimal(rounding.trim().replace(/^-/, ''), currency),
      },
      reviewed,
    };
  }
  async function save(reviewed: boolean) {
    setBusy(true);
    setError('');
    try {
      const review = await api.saveReceiptItems(input(reviewed));
      setSaved(review);
      setDirty(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const summary = saved.reconciliation;
  return (
    <Panel
      title="Receipt line items"
      action={
        <span className={`badge ${saved.reviewed && !dirty ? 'ready' : 'needs_review'}`}>
          {saved.reviewed && !dirty ? 'Reviewed' : 'Needs review'}
        </span>
      }
    >
      <p className="muted">
        Amounts use the original receipt currency ({currency}). Save receipt details above first.
        Line items stay separate from the ledger; recording this receipt creates one expense.
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save(true);
        }}
        onChange={() => setDirty(true)}
      >
        <fieldset disabled={busy || locked}>
          <Field label="Line-item currency (match the original receipt)">
            <select value={currency} onChange={(event) => setCurrency(event.target.value)}>
              {CURRENCIES.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </Field>
          {!items.length && (
            <p>No items were extracted. Add the printed purchases to reconcile this receipt.</p>
          )}
          {items.map((item, index) => (
            <div className="finance-budget-body" key={index}>
              <Field label={`Line ${index + 1} description`}>
                <input
                  required
                  maxLength={500}
                  value={item.description}
                  onChange={(event) => line(index, { description: event.target.value })}
                />
              </Field>
              <div className="finance-card-grid">
                <Field label={`Line ${index + 1} quantity (optional)`}>
                  <input
                    inputMode="decimal"
                    value={item.quantity}
                    onChange={(event) => line(index, { quantity: event.target.value })}
                  />
                </Field>
                <Field label={`Line ${index + 1} unit price (${currency}, optional)`}>
                  <input
                    inputMode="decimal"
                    value={item.unitPrice}
                    onChange={(event) => line(index, { unitPrice: event.target.value })}
                  />
                </Field>
                <Field label={`Line ${index + 1} net amount (${currency})`}>
                  <input
                    required
                    inputMode="decimal"
                    value={item.amount}
                    onChange={(event) => line(index, { amount: event.target.value })}
                  />
                </Field>
              </div>
              <div className="finance-row-actions">
                <span className="muted">
                  {saved.source === 'manual'
                    ? 'Manually saved values'
                    : `${Math.round(item.confidence * 100)}% confidence · verify against receipt`}
                </span>
                <button
                  type="button"
                  className="text-button danger-text"
                  onClick={() => {
                    setItems((previous) => previous.filter((_, row) => row !== index));
                    setDirty(true);
                  }}
                >
                  Remove line {index + 1}
                </button>
              </div>
            </div>
          ))}
          <button
            type="button"
            className="button secondary"
            disabled={items.length >= 200}
            onClick={() => {
              setItems((previous) => [
                ...previous,
                { description: '', quantity: '', unitPrice: '', amount: '', confidence: 1 },
              ]);
              setDirty(true);
            }}
          >
            Add line item
          </button>
          <div className="finance-card-grid">
            <Field label={`Receipt tax (${currency})`}>
              <input
                inputMode="decimal"
                value={tax}
                onChange={(event) => setTax(event.target.value)}
              />
            </Field>
            <Field label={`Receipt-level discount (${currency})`}>
              <input
                inputMode="decimal"
                value={discount}
                onChange={(event) => setDiscount(event.target.value)}
              />
            </Field>
            <Field label={`Service charge (${currency})`}>
              <input
                inputMode="decimal"
                value={service}
                onChange={(event) => setService(event.target.value)}
              />
            </Field>
            <Field label={`Rounding adjustment (${currency}, signed)`}>
              <input
                inputMode="decimal"
                value={rounding}
                onChange={(event) => setRounding(event.target.value)}
              />
            </Field>
          </div>
          <label className="check-field">
            <input
              type="checkbox"
              checked={included}
              onChange={(event) => setIncluded(event.target.checked)}
            />{' '}
            Line amounts already include tax
          </label>
          <p className="muted">
            Printed net line amounts already include line-level discounts. Add only receipt-level
            adjustments here.
          </p>
          <QueryError error={error} />
          <div className="modal-actions">
            <button className="button secondary" type="button" onClick={() => void save(false)}>
              Save line-item draft
            </button>
            <button className="button primary" type="submit">
              {busy ? 'Saving…' : 'Confirm line-item review'}
            </button>
          </div>
        </fieldset>
      </form>
      {locked && <p className="notice-box">Reopen this claim to edit receipt line items.</p>}
      {dirty ? (
        <p className="notice-box">Unsaved changes. Save a draft to recalculate reconciliation.</p>
      ) : (
        <div
          className="finance-budget-body"
          role="status"
          aria-label="Receipt line-item reconciliation"
        >
          <p>
            Items: {signedMoney(summary.itemsSubtotalMinor, currency)} · With adjustments:{' '}
            {signedMoney(summary.calculatedTotalMinor, currency)}
          </p>
          <p>
            Original receipt total:{' '}
            {summary.receiptTotalMinor === null
              ? 'not set'
              : signedMoney(summary.receiptTotalMinor, currency)}{' '}
            · Difference:{' '}
            {summary.differenceMinor === null
              ? 'not available'
              : signedMoney(summary.differenceMinor, currency)}
          </p>
          {summary.warnings.map((warning) => (
            <p className="muted" key={warning}>
              {warning}
            </p>
          ))}
          {summary.balanced && <p>Line items reconcile with the original receipt total.</p>}
        </div>
      )}
    </Panel>
  );
}
export function ReceiptItems({
  expenseId,
  expenseVersion,
  locked = false,
}: {
  expenseId: string;
  expenseVersion: number;
  locked?: boolean;
}) {
  const loader = useCallback(() => {
    void expenseVersion;
    return api.receiptItems(expenseId);
  }, [expenseId, expenseVersion]);
  const query = useQuery(loader);
  return (
    <>
      <QueryError error={query.error} />
      {query.loading ? (
        <Loading />
      ) : (
        query.data && (
          <ReviewEditor
            key={`${query.data.expenseId}:${query.data.version}:${query.data.expenseVersion}`}
            initial={query.data}
            locked={locked}
          />
        )
      )}
    </>
  );
}
