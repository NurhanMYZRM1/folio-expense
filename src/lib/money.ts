export const CURRENCIES = [
  'MYR',
  'USD',
  'SGD',
  'EUR',
  'GBP',
  'AUD',
  'CAD',
  'CHF',
  'CNY',
  'HKD',
  'INR',
  'THB',
  'IDR',
  'JPY',
  'KRW',
  'BHD',
  'KWD',
  'OMR',
] as const;
export const MAX_MONEY = 999_999_999_999;
export function exponent(currency: string): number {
  if (!(CURRENCIES as readonly string[]).includes(currency))
    throw new Error('Unsupported currency.');
  return ['JPY', 'KRW'].includes(currency) ? 0 : ['BHD', 'KWD', 'OMR'].includes(currency) ? 3 : 2;
}
export function parseMoney(input: string, currency: string): number | null {
  if (!input.trim()) return null;
  const precision = exponent(currency);
  const value = input.trim();
  if (!/^\d+(?:\.\d*)?$/.test(value))
    throw new Error('Enter an amount using digits and a decimal point.');
  const [whole, fraction = ''] = value.split('.');
  if (fraction.length > precision)
    throw new Error(`${currency} supports ${precision} decimal places.`);
  const minor =
    BigInt(whole) * 10n ** BigInt(precision) + BigInt(fraction.padEnd(precision, '0') || '0');
  if (minor > BigInt(MAX_MONEY)) throw new Error('Amount is too large.');
  return Number(minor);
}
export function moneyInput(minor: number | null, currency: string): string {
  if (minor === null) return '';
  const digits = exponent(currency),
    divisor = 10 ** digits;
  if (!Number.isSafeInteger(minor) || minor < 0) throw new Error('Invalid money value.');
  return `${Math.floor(minor / divisor)}${digits ? `.${String(minor % divisor).padStart(digits, '0')}` : ''}`;
}
export function formatMoney(minor: number | null, currency = 'MYR'): string {
  if (minor === null) return '—';
  const raw = moneyInput(minor, currency),
    [whole, fraction] = raw.split('.');
  return `${currency} ${whole.replace(/\B(?=(\d{3})+(?!\d))/g, ',')}${fraction === undefined ? '' : `.${fraction}`}`;
}
export function sumMinor(values: (number | null)[]): number {
  const sum = values.reduce<number>((a, b) => a + (b ?? 0), 0);
  if (!Number.isSafeInteger(sum)) throw new Error('Total is outside the safe range.');
  return sum;
}
