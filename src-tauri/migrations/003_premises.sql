-- Where the purchase was made: the address printed on the receipt, read by
-- OCR or online AI, or entered by hand.
ALTER TABLE expenses ADD COLUMN premises TEXT CHECK(premises IS NULL OR length(premises) <= 500);
PRAGMA user_version=3;
