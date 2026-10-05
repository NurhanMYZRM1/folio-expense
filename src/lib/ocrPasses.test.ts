import { describe, it, expect } from 'vitest';
import { bestPass, hasTotalAmount, isGoodPass } from './ocrPasses';

// Real first-pass text of a tilted Singapore receipt: the price column was dropped.
const dropped = `YA KUN KAYA TOAST PTE LTD
30 Sep 2026 09:10
Kaya Toast Set A
SUBTOTAL
GST 9%
TOTAL S$
NETS`;
// The uniform-block pass of the same photo.
const recovered = `YA KUN KAYA TOAST PTE LTD
30 Sep 2026 09:10
Kaya Toast Set A          6.40
SUBTOTAL                  8.50
GST 9%                    0.77
TOTAL S$                  9.27
NETS                      9.27`;

describe('hasTotalAmount', () => {
  it('needs an amount beside the total label', () => {
    expect(hasTotalAmount(dropped)).toBe(false);
    expect(hasTotalAmount(recovered)).toBe(true);
  });
  it('accepts the amount on the following line', () => {
    expect(hasTotalAmount('GRAND TOTAL\nRM 59.00')).toBe(true);
    expect(hasTotalAmount('TOTAL\nThank you')).toBe(false);
  });
  it('does not count a subtotal as the total', () => {
    expect(hasTotalAmount('SUBTOTAL 24.00\nTOTAL')).toBe(false);
  });
});

describe('bestPass', () => {
  it('keeps the pass that read the total, even at lower confidence', () => {
    const passes = [
      { text: dropped, confidence: 95 },
      { text: recovered, confidence: 88 },
    ];
    expect(bestPass(passes).text).toBe(recovered);
  });
  it('prefers some text over an empty reading', () => {
    expect(
      bestPass([
        { text: '', confidence: 95 },
        { text: 'KOPI\n12.00', confidence: 55 },
      ]).text,
    ).toBe('KOPI\n12.00');
  });
  it('treats a confident reading with a total as good enough', () => {
    expect(isGoodPass({ text: recovered, confidence: 90 })).toBe(true);
    expect(isGoodPass({ text: recovered, confidence: 50 })).toBe(false);
    expect(isGoodPass({ text: dropped, confidence: 95 })).toBe(false);
  });
});
