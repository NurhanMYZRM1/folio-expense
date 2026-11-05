import { useCallback, useState, type FormEvent } from 'react';
import type {
  IncomeTaxEstimate,
  MaritalStatus,
  SstCalculation,
  TaxRules,
} from '../../bindings/generated';
import { Loading, Panel } from '../../components/ui';
import { errorMessage } from '../../lib/errors';
import { api } from '../../lib/ipc';
import { formatMoney, moneyInput, parseMoney } from '../../lib/money';
import { Field, FinanceFrame, QueryError, signedMoney, useQuery } from './shared';

function nonnegative(value: string) {
  return parseMoney(value, 'MYR') ?? 0;
}
function IncomeEstimate({ rules }: { rules: TaxRules }) {
  const [income, setIncome] = useState('');
  const [marital, setMarital] = useState<MaritalStatus>('single');
  const [claims, setClaims] = useState<Record<string, { amount: string; confirmed: boolean }>>({});
  const [zakat, setZakat] = useState('');
  const [pcb, setPcb] = useState('');
  const [result, setResult] = useState<IncomeTaxEstimate | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [resident, setResident] = useState(false);
  async function calculate(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError('');
    setResult(null);
    try {
      if (!resident)
        throw new Error(
          'Confirm this estimate matches your tax residency and supported assessment scope.',
        );
      const estimate = await api.estimateIncomeTax({
        assessmentYear: rules.assessmentYear,
        resident,
        maritalStatus: marital,
        totalIncomeMinor: nonnegative(income),
        reliefs: Object.entries(claims)
          .filter(([, claim]) => claim.amount.trim())
          .map(([code, claim]) => ({
            code,
            amountMinor: nonnegative(claim.amount),
            eligibilityConfirmed: claim.confirmed,
          })),
        zakatMinor: nonnegative(zakat),
        pcbPaidMinor: nonnegative(pcb),
      });
      setResult(estimate);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Panel title={`Resident income tax · YA ${rules.assessmentYear}`}>
      <p className="notice-box">
        Estimate only, calculated on your device. YA 2026 is not supported. No tax return is filed.
      </p>
      <p className="muted">{rules.supportedScope}</p>
      <form onSubmit={(event) => void calculate(event)} onChange={() => setResult(null)}>
        <fieldset disabled={busy}>
          <Field label="Total income before personal reliefs (MYR)">
            <input
              required
              inputMode="decimal"
              value={income}
              onChange={(event) => setIncome(event.target.value)}
            />
          </Field>
          <p className="muted">
            Enter total income after permitted deductions. This is not gross salary or business
            revenue.
          </p>
          <Field label="Assessment status">
            <select
              value={marital}
              onChange={(event) => setMarital(event.target.value as MaritalStatus)}
            >
              <option value="single">Single / separate individual assessment</option>
              <option value="married_separate">
                Married · separate assessment, spouse has income
              </option>
              <option value="married_spouse_no_income">
                Married · eligible spouse with no income
              </option>
            </select>
          </Field>
          <p>
            Individual relief: MYR 9,000.00.{' '}
            {marital === 'married_spouse_no_income' && 'Eligible spouse relief: MYR 4,000.00.'}
          </p>
          <details>
            <summary>Eligible personal reliefs</summary>
            <p className="muted">
              Enter qualifying expenditure. Folio caps each claim. Confirm eligibility using the
              source and allocate each payment once.
            </p>
            {rules.reliefs
              .filter((rule) => !['individual', 'spouse'].includes(rule.code))
              .map((rule) => {
                const claim = claims[rule.code] ?? { amount: '', confirmed: false };
                return (
                  <div className="finance-budget-body" key={rule.code}>
                    <Field label={`${rule.label} (MYR; cap ${moneyInput(rule.capMinor, 'MYR')})`}>
                      <input
                        inputMode="decimal"
                        value={claim.amount}
                        onChange={(event) =>
                          setClaims((previous) => ({
                            ...previous,
                            [rule.code]: { ...claim, amount: event.target.value },
                          }))
                        }
                      />
                    </Field>
                    <p className="muted">{rule.eligibility}</p>
                    <label className="check-field">
                      <input
                        type="checkbox"
                        checked={claim.confirmed}
                        onChange={(event) =>
                          setClaims((previous) => ({
                            ...previous,
                            [rule.code]: { ...claim, confirmed: event.target.checked },
                          }))
                        }
                      />{' '}
                      I checked eligibility and excluded duplicate claims.
                    </label>
                  </div>
                );
              })}
          </details>
          <Field label="Qualifying zakat paid (MYR)">
            <input
              inputMode="decimal"
              value={zakat}
              onChange={(event) => setZakat(event.target.value)}
            />
          </Field>
          <Field label="PCB / MTD already withheld for YA 2025 (MYR)">
            <input
              inputMode="decimal"
              value={pcb}
              onChange={(event) => setPcb(event.target.value)}
            />
          </Field>
          <p className="muted">
            PCB is deducted from estimated annual tax. Folio does not calculate monthly payroll
            withholding.
          </p>
          <label className="check-field">
            <input
              type="checkbox"
              required
              checked={resident}
              onChange={(event) => setResident(event.target.checked)}
            />{' '}
            I am a Malaysian tax resident and my inputs fit the supported assessment scope.
          </label>
          <QueryError error={error} />
          <div className="modal-actions">
            <button className="button primary" type="submit">
              {busy ? 'Calculating…' : 'Estimate annual tax'}
            </button>
          </div>
        </fieldset>
      </form>
      {result && (
        <div className="finance-budget-body" role="status" aria-label="Annual tax estimate">
          <dl>
            <dt>Personal reliefs</dt>
            <dd>{formatMoney(result.reliefMinor, 'MYR')}</dd>
            <dt>Chargeable income</dt>
            <dd>{formatMoney(result.chargeableIncomeMinor, 'MYR')}</dd>
            <dt>Tax before rebates</dt>
            <dd>{formatMoney(result.taxBeforeRebatesMinor, 'MYR')}</dd>
            <dt>Rebates applied</dt>
            <dd>{formatMoney(result.rebateMinor, 'MYR')}</dd>
            <dt>Estimated tax payable</dt>
            <dd>{formatMoney(result.taxPayableMinor, 'MYR')}</dd>
            <dt>
              {result.balanceAfterPcbMinor < 0
                ? 'Estimated overpayment after PCB'
                : 'Estimated balance after PCB'}
            </dt>
            <dd>{formatMoney(Math.abs(result.balanceAfterPcbMinor), 'MYR')}</dd>
          </dl>
          {result.warnings.map((warning) => (
            <p className="muted" key={warning}>
              {warning}
            </p>
          ))}
        </div>
      )}
      <p className="muted">
        Verified sources:{' '}
        {rules.sources.map((source, index) => (
          <span key={source}>
            {index > 0 && ' · '}
            <a href={source} target="_blank" rel="noreferrer">
              {['LHDN rates', 'LHDN reliefs', 'Form BE notes'][index] ?? 'LHDN guidance'}
            </a>
          </span>
        ))}
      </p>
    </Panel>
  );
}
function SstEstimate() {
  const [amount, setAmount] = useState('');
  const [rate, setRate] = useState(600);
  const [inclusive, setInclusive] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [result, setResult] = useState<SstCalculation | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  async function calculate(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError('');
    setResult(null);
    try {
      if (!confirmed)
        throw new Error('Confirm the applicable rate and tax treatment before calculating.');
      setResult(
        await api.calculateSst({
          amountMinor: nonnegative(amount),
          rateBasisPoints: rate,
          taxInclusive: inclusive,
        }),
      );
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Panel title="SST arithmetic helper">
      <p className="muted">
        Select a rate you have confirmed with Customs. Folio does not determine taxable goods,
        registration, exemptions or effective-date rules.
      </p>
      <form onSubmit={(event) => void calculate(event)} onChange={() => setResult(null)}>
        <fieldset disabled={busy}>
          <Field label="Amount (MYR)">
            <input
              required
              inputMode="decimal"
              value={amount}
              onChange={(event) => setAmount(event.target.value)}
            />
          </Field>
          <Field label="Confirmed rate">
            <select value={rate} onChange={(event) => setRate(Number(event.target.value))}>
              {[0, 500, 600, 800, 1000].map((value) => (
                <option key={value} value={value}>
                  {value / 100}%
                </option>
              ))}
            </select>
          </Field>
          <label className="check-field">
            <input
              type="checkbox"
              checked={inclusive}
              onChange={(event) => setInclusive(event.target.checked)}
            />{' '}
            Amount already includes tax
          </label>
          <label className="check-field">
            <input
              type="checkbox"
              required
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />{' '}
            I checked the applicable rate and treatment.
          </label>
          <QueryError error={error} />
          <div className="modal-actions">
            <button type="submit" className="button primary">
              {busy ? 'Calculating…' : 'Calculate SST'}
            </button>
          </div>
        </fieldset>
      </form>
      {result && (
        <div className="finance-budget-body" role="status" aria-label="SST calculation">
          <p>
            Net: {signedMoney(result.netMinor, 'MYR')} · Tax: {signedMoney(result.taxMinor, 'MYR')}{' '}
            · Total: {signedMoney(result.totalMinor, 'MYR')}
          </p>
          <p className="muted">{result.notice}</p>
          <a href={result.sourceUrl} target="_blank" rel="noreferrer">
            Customs guidance
          </a>
        </div>
      )}
    </Panel>
  );
}
export function TaxTools() {
  const query = useQuery(useCallback(() => api.taxRules(2025), []));
  return (
    <FinanceFrame
      title="Malaysian tax helpers"
      subtitle="Source-verified estimates and arithmetic for preparation. Nothing is submitted."
    >
      <QueryError error={query.error} />
      {query.loading ? <Loading /> : query.data && <IncomeEstimate rules={query.data} />}
      <SstEstimate />
    </FinanceFrame>
  );
}
