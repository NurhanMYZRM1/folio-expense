//! Personal and business finance ledger types shared with the UI (exported to
//! `src/bindings`). Rules live in `services::finance`; see docs/DECISIONS.md.
//! Amounts are integer minor units of the stated currency (D3); dates are
//! local calendar dates `YYYY-MM-DD` and periods calendar months `YYYY-MM` (D4).
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Personal,
    Business,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub kind: WorkspaceKind,
    pub name: String,
    pub base_currency: String,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub sync_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceInput {
    /// None creates a business workspace (the personal one always exists).
    pub id: Option<String>,
    pub version: Option<i32>,
    pub name: String,
    /// Changing it re-converts every transaction's base amount.
    pub base_currency: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CategoryKind {
    Income,
    Expense,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub workspace_id: String,
    pub kind: CategoryKind,
    pub name: String,
    pub archived_at: Option<String>,
    pub version: i32,
    pub sync_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub kind: CategoryKind,
    pub name: String,
    pub archived: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Cash,
    Bank,
    Ewallet,
    Card,
    /// Holds a savings goal when `goal_target_minor` is set.
    Savings,
    Loan,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LoanDirection {
    /// You owe the money: the account balance is negative.
    Borrowed,
    /// You are owed the money: the account balance is positive.
    Lent,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub kind: AccountKind,
    pub currency: String,
    #[ts(type = "number")]
    pub opening_balance_minor: i64,
    /// Opening balance plus every posted transaction (computed, never stored).
    #[ts(type = "number")]
    pub balance_minor: i64,
    #[ts(type = "number | null")]
    pub goal_target_minor: Option<i64>,
    pub goal_target_date: Option<String>,
    pub loan_direction: Option<LoanDirection>,
    #[ts(type = "number | null")]
    pub loan_principal_minor: Option<i64>,
    /// Informational annual percentage rate as typed, e.g. "4.35".
    pub loan_annual_rate: Option<String>,
    pub counterparty: Option<String>,
    pub note: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub sync_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub name: String,
    pub kind: AccountKind,
    /// Fixed once the account has transactions or bills.
    pub currency: String,
    #[ts(type = "number")]
    pub opening_balance_minor: i64,
    #[ts(type = "number | null")]
    pub goal_target_minor: Option<i64>,
    pub goal_target_date: Option<String>,
    pub loan_direction: Option<LoanDirection>,
    #[ts(type = "number | null")]
    pub loan_principal_minor: Option<i64>,
    pub loan_annual_rate: Option<String>,
    pub counterparty: Option<String>,
    pub note: String,
    /// Only a zero-balance account can be archived.
    pub archived: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    Income,
    Expense,
    Transfer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    DebitCard,
    CreditCard,
    Ewallet,
    BankTransfer,
    Duitnow,
    Fpx,
    Cheque,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TransactionSource {
    Manual,
    Receipt,
    QuickAdd,
    Voice,
    Bill,
    Invoice,
    Notification,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: String,
    pub workspace_id: String,
    pub kind: TransactionKind,
    pub account_id: String,
    pub account_name: String,
    /// The account's currency.
    pub currency: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub to_account_id: Option<String>,
    pub to_account_name: Option<String>,
    pub to_currency: Option<String>,
    #[ts(type = "number | null")]
    pub to_amount_minor: Option<i64>,
    /// In the workspace base currency; null while awaiting an exchange rate.
    #[ts(type = "number | null")]
    pub base_amount_minor: Option<i64>,
    pub exchange_rate: Option<String>,
    pub exchange_rate_date: Option<String>,
    pub occurred_on: String,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub payee: Option<String>,
    pub note: String,
    pub payment_method: Option<PaymentMethod>,
    pub source: TransactionSource,
    /// The reviewed receipt this records; it owns amount, date and payee.
    pub expense_id: Option<String>,
    pub receipt_id: Option<String>,
    pub bill_id: Option<String>,
    pub bill_due_on: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub sync_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub kind: TransactionKind,
    pub account_id: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub to_account_id: Option<String>,
    /// Required for transfers; must equal `amount_minor` between accounts of
    /// the same currency.
    #[ts(type = "number | null")]
    pub to_amount_minor: Option<i64>,
    pub occurred_on: String,
    pub category_id: Option<String>,
    pub payee: Option<String>,
    pub note: String,
    pub payment_method: Option<PaymentMethod>,
    /// How the values were captured (creation only; ignored on update).
    pub source: TransactionSource,
}

/// Records a reviewed receipt (expense) in the ledger. Amount, date and payee
/// come from the receipt; the account currency must match its claimed currency.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordExpenseInput {
    pub expense_id: String,
    pub account_id: String,
    pub category_id: Option<String>,
    pub payment_method: Option<PaymentMethod>,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionFilter {
    pub workspace_id: String,
    /// Inclusive date bounds.
    pub from: Option<String>,
    pub to: Option<String>,
    /// Matches either side of a transfer.
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub kind: Option<TransactionKind>,
    /// Case-insensitive match on payee and note.
    pub search: Option<String>,
    #[ts(type = "number | null")]
    pub min_amount_minor: Option<i64>,
    #[ts(type = "number | null")]
    pub max_amount_minor: Option<i64>,
    /// 1..=500.
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransactionPage {
    pub items: Vec<Transaction>,
    /// Matching rows before limit/offset.
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub id: String,
    pub workspace_id: String,
    /// None: all spending.
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    /// Per calendar month, in the workspace base currency.
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub alert_percent: i32,
    pub archived_at: Option<String>,
    pub version: i32,
    pub sync_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub category_id: Option<String>,
    #[ts(type = "number")]
    pub amount_minor: i64,
    /// 1..=100; an alert also fires at 100%.
    pub alert_percent: i32,
    pub archived: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    OnTrack,
    /// At or above `alert_percent`, not yet over.
    NearLimit,
    Over,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BudgetProgress {
    pub budget: Budget,
    pub period: String,
    pub base_currency: String,
    #[ts(type = "number")]
    pub spent_minor: i64,
    /// Negative when over budget.
    #[ts(type = "number")]
    pub remaining_minor: i64,
    /// floor(spent × 100 / amount); may exceed 100.
    pub percent: i32,
    pub status: BudgetStatus,
    /// Expenses in the period still awaiting an exchange rate (not counted).
    pub unconverted_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BillKind {
    Bill,
    Subscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Once,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Bill {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub kind: BillKind,
    #[ts(type = "number")]
    pub amount_minor: i64,
    /// The paying account's currency.
    pub currency: String,
    pub account_id: String,
    pub category_id: Option<String>,
    pub payee: Option<String>,
    pub note: String,
    pub frequency: Frequency,
    pub interval_count: i32,
    pub anchor_on: String,
    pub ends_on: Option<String>,
    pub remind_days_before: i32,
    pub reminders_enabled: bool,
    pub cancelled_on: Option<String>,
    /// Earliest unpaid occurrence (may be overdue); None when none remain.
    pub next_due_on: Option<String>,
    pub version: i32,
    pub sync_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BillInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub name: String,
    pub kind: BillKind,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub account_id: String,
    pub category_id: Option<String>,
    pub payee: Option<String>,
    pub note: String,
    pub frequency: Frequency,
    pub interval_count: i32,
    /// The next due date; saving re-anchors the schedule here (D4).
    pub anchor_on: String,
    pub ends_on: Option<String>,
    pub remind_days_before: i32,
    pub reminders_enabled: bool,
    pub cancelled_on: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceStatus {
    Paid,
    Overdue,
    DueToday,
    Upcoming,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BillOccurrence {
    pub bill_id: String,
    pub bill_name: String,
    pub kind: BillKind,
    pub due_on: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: String,
    pub status: OccurrenceStatus,
    /// The payment, when paid.
    pub transaction_id: Option<String>,
    #[ts(type = "number | null")]
    pub paid_amount_minor: Option<i64>,
    /// When the reminder fires (09:00 local); None when reminders are off.
    pub remind_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BillDetail {
    pub bill: Bill,
    /// Up to 12 past and 3 future occurrences, oldest first.
    pub occurrences: Vec<BillOccurrence>,
}

/// Pays one occurrence. Idempotent: paying it again returns the existing payment.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BillPaymentInput {
    pub bill_id: String,
    /// Must be an occurrence of the bill's schedule.
    pub due_on: String,
    /// Defaults to the bill's amount.
    #[ts(type = "number | null")]
    pub amount_minor: Option<i64>,
    pub paid_on: String,
    /// Defaults to the bill's account.
    pub account_id: Option<String>,
    pub payment_method: Option<PaymentMethod>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    /// None: uncategorised.
    pub category_id: Option<String>,
    pub name: String,
    pub kind: CategoryKind,
    #[ts(type = "number")]
    pub total_minor: i64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DailyTotal {
    pub date: String,
    #[ts(type = "number")]
    pub income_minor: i64,
    #[ts(type = "number")]
    pub expense_minor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyReport {
    pub workspace_id: String,
    pub period: String,
    pub base_currency: String,
    #[ts(type = "number")]
    pub income_minor: i64,
    #[ts(type = "number")]
    pub expense_minor: i64,
    #[ts(type = "number")]
    pub net_minor: i64,
    /// Same totals for the month before, for comparison.
    #[ts(type = "number")]
    pub previous_income_minor: i64,
    #[ts(type = "number")]
    pub previous_expense_minor: i64,
    /// Expense categories first, each kind sorted by total descending.
    pub by_category: Vec<CategoryTotal>,
    /// Every day of the month, including days without transactions.
    pub daily: Vec<DailyTotal>,
    /// Income/expense rows awaiting an exchange rate (excluded above).
    pub unconverted_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyAmount {
    pub currency: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub workspace: Workspace,
    pub today: String,
    /// Sum of non-loan, unarchived accounts in the base currency.
    #[ts(type = "number")]
    pub total_balance_minor: i64,
    /// Non-loan account balances in other currencies, unconverted.
    pub other_balances: Vec<CurrencyAmount>,
    /// This calendar month, in the base currency.
    #[ts(type = "number")]
    pub month_income_minor: i64,
    #[ts(type = "number")]
    pub month_expense_minor: i64,
    pub accounts: Vec<Account>,
    pub budgets: Vec<BudgetProgress>,
    /// Overdue occurrences plus those due in the next 14 days.
    pub bills: Vec<BillOccurrence>,
    /// The 8 most recent transactions.
    pub recent: Vec<Transaction>,
    pub pending_suggestions: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    Bill,
    Budget,
}

/// Something the device should notify about. `id` is stable, so clients can
/// cancel and reschedule every reminder idempotently.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    /// `bill:<bill id>:<due date>` or `budget:<budget id>:<period>:<threshold>`.
    pub id: String,
    pub kind: ReminderKind,
    pub workspace_id: String,
    pub title: String,
    pub body: String,
    /// Local date to notify on, at 09:00 local time. Budget alerts are due now.
    pub fire_on: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Dismissed,
}

/// A transaction proposed from an Android payment notification.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransactionSuggestion {
    pub id: String,
    pub workspace_id: String,
    pub kind: TransactionKind,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: String,
    pub occurred_on: String,
    pub payee: Option<String>,
    /// The bank or e-wallet, e.g. "Maybank".
    pub app_label: String,
    pub status: SuggestionStatus,
    pub transaction_id: Option<String>,
    pub created_at: String,
}

/// A parsed quick-add or voice phrase for the user to review before saving.
/// Nothing is stored until it is saved as a transaction.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QuickAddDraft {
    pub text: String,
    pub kind: TransactionKind,
    #[ts(type = "number | null")]
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub occurred_on: String,
    pub category_id: Option<String>,
    pub account_id: Option<String>,
    pub payee: Option<String>,
    pub note: String,
    /// Fields the parser was unsure of (`amount`, `date`, `category`,
    /// `account`, `kind`, `payee`); the review form highlights them.
    pub uncertain: Vec<String>,
}
