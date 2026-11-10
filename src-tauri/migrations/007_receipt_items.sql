-- Optional receipt item review; original expenses/claims/attachments unchanged.
CREATE TABLE receipt_item_reviews (
 expense_id TEXT PRIMARY KEY REFERENCES expenses(id) ON DELETE CASCADE,
 data TEXT NOT NULL CHECK(json_valid(data))
);
PRAGMA user_version=7;
