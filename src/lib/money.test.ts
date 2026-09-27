import { describe, it, expect } from 'vitest';
import { parseMoney, formatMoney, moneyInput, sumMinor } from './money';
describe('integer minor units', () => {
  it('parses without binary floating point rounding', () => {
    expect(parseMoney('0.29', 'MYR')).toBe(29);
    expect(parseMoney('42.50', 'MYR')).toBe(4250);
    expect(parseMoney('9999999999.99', 'MYR')).toBe(999999999999);
  });
  it('honors zero- and three-decimal currencies', () => {
    expect(parseMoney('127', 'JPY')).toBe(127);
    expect(parseMoney('1.234', 'KWD')).toBe(1234);
    expect(moneyInput(1234, 'KWD')).toBe('1.234');
  });
  it('rejects invalid, overprecise and out-of-range input', () => {
    for (const value of ['-1', '1e5', '12,00', '1.234', 'NaN', '10000000000'])
      expect(() => parseMoney(value, 'MYR')).toThrow();
    expect(() => parseMoney('1.1', 'JPY')).toThrow();
  });
  it('formats, sums, and preserves missing values', () => {
    expect(parseMoney('', 'MYR')).toBeNull();
    expect(formatMoney(null)).toBe('—');
    expect(formatMoney(123450)).toBe('MYR 1,234.50');
    expect(sumMinor([29, 71, null])).toBe(100);
    expect(() => sumMinor([Number.MAX_SAFE_INTEGER, 1])).toThrow();
  });
});
