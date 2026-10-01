-- Foreign-currency receipts converted to the home currency. When set, the
-- original_* columns hold the amounts printed on the receipt and the main
-- currency/total/tax columns hold the converted (claimed) amounts.
ALTER TABLE expenses ADD COLUMN original_currency TEXT;
ALTER TABLE expenses ADD COLUMN original_total_amount_minor INTEGER CHECK(original_total_amount_minor BETWEEN 0 AND 999999999999);
ALTER TABLE expenses ADD COLUMN original_tax_amount_minor INTEGER CHECK(original_tax_amount_minor BETWEEN 0 AND 999999999999);
-- Units of the home currency per one unit of the original currency, exactly as published.
ALTER TABLE expenses ADD COLUMN exchange_rate TEXT;
ALTER TABLE expenses ADD COLUMN exchange_rate_date TEXT;
-- Published rates already downloaded, so each is fetched once and works offline afterwards.
CREATE TABLE exchange_rates (
 base TEXT NOT NULL, quote TEXT NOT NULL, requested_date TEXT NOT NULL,
 rate_date TEXT NOT NULL, rate TEXT NOT NULL, source TEXT NOT NULL, fetched_at TEXT NOT NULL,
 PRIMARY KEY(base, quote, requested_date)
);
PRAGMA user_version=2;
