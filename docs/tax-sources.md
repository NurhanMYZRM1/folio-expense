# Malaysian tax helpers: supported rules and sources

Retrieved 2026-11-05 from primary sources. The original worker's notes were
read as historical research; sources below were opened again before coding.
Nothing is submitted to LHDN, MyInvois or Customs.

## Resident income-tax estimate

Only YA 2025 is enabled. The [LHDN rate table](https://www.hasil.gov.my/en/individu/kadar-cukai/)
lists YA 2023–2025, but no YA 2026 schedule. The encoded upper band boundaries
(RM) and marginal percentages are:

| Upper boundary  | Rate |
| --------------- | ---- |
| 5,000           | 0%   |
| 20,000          | 1%   |
| 35,000          | 3%   |
| 50,000          | 6%   |
| 70,000          | 11%  |
| 100,000         | 19%  |
| 400,000         | 25%  |
| 600,000         | 26%  |
| 2,000,000       | 28%  |
| Above 2,000,000 | 30%  |

The [YA 2025 Form BE notes](https://ef.hasil.gov.my/eBE2026/Pdf/Nota_BE_e.pdf),
page 9 (B18/B21), give the test example RM45,500 chargeable income → RM1,230
tax. Self rebate is RM400 at chargeable income ≤ RM35,000, with a further
RM400 when the spouse deduction is allowed. Zakat rebates are limited to tax
charged. Page 12 (B28) subtracts MTD/PCB payments from tax payable. No payroll
PCB forecast is implemented.

The [LHDN YA 2025 relief table](https://www.hasil.gov.my/en/individu/pelepasan-cukai/)
provides the implemented caps: individual RM9,000; spouse RM4,000; disabled
individual RM7,000; lifestyle RM2,500; qualifying EPF RM4,000 plus life
insurance/family takaful/additional voluntary EPF RM3,000; PRS RM3,000;
education/medical insurance RM4,000; SOCSO/EIS RM350. Each claim carries
user confirmation of eligibility and is capped. EPF allocations must not be
double-counted. Individual/spouse amounts are automatically included only
for the supported assessment choice.

Folio decisions: calculate with integer sen and round the final band tax
half up to sen; this is an estimation convention, not a filing instruction.
Input is total income after permitted deductions and before personal reliefs,
not gross salary or revenue. Unsupported: non-residents, joint assessment,
alimony, child relief, other unlisted reliefs, dividend-specific tax, foreign
credits, special rates and business-income computation. The result explains
this scope and never labels itself a filed return.

## SST arithmetic

[Royal Malaysian Customs background](https://mysst.customs.gov.my/background/)
(last updated 19 July 2026) describes 8% service tax from 1 March 2024,
with listed 6% services and specific-rate exceptions; later scope and rate
changes mean the app must not infer a rate from an item name.
[Customs sales-tax FAQ](https://mysst.customs.gov.my/faq-sales-tax/) lists 5%,
10% and specific rates.

Folio's helper supports user-confirmed 0/5/6/8/10%, inclusive or exclusive,
using integer half-up arithmetic. Invoice tax is likewise a user-confirmed
basis-point input. These calculations do not determine registration,
exemptions, classification, tax-point or transition treatment. No SST return
is generated or submitted. Specific per-unit taxes are unsupported.

## MyInvois preparation

The [official Invoice v1.1 structure](https://sdk.myinvois.hasil.gov.my/documents/invoice-v1-1/)
requires supplier, buyer, invoice number, currency, line information, UTC
issue date/time and a digital signature. Folio provides a **local checklist**
for available identity, address, number and classification fields. It does
not create a compliant UBL document or validate a TIN, code-list membership,
signature or government eligibility. Every result has status
`local_preflight_only` and explains the missing official validation. There
is no network submission operation and no claim that a preflight pass is
MyInvois acceptance.
