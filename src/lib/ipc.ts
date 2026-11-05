import { invoke, isTauri } from '@tauri-apps/api/core';
import type {
  Account,
  AccountInput,
  AppInfo,
  Bill,
  BillDetail,
  BillInput,
  BillOccurrence,
  BillPaymentInput,
  BudgetInput,
  Budget,
  BudgetProgress,
  Category,
  CategoryInput,
  MonthlyReport,
  Overview,
  QuickAddDraft,
  RecordExpenseInput,
  Reminder,
  Transaction,
  TransactionFilter,
  TransactionInput,
  TransactionPage,
  TransactionSuggestion,
  Workspace,
  WorkspaceInput,
  Claim,
  ClaimDetail,
  ClaimStatus,
  CsvExport,
  Expense,
  ExpenseEdit,
  ExportSnapshot,
  ImportOutcome,
  Job,
  JobLease,
  ReceiptContent,
  Settings,
} from '../bindings/generated';
export const desktop = isTauri();
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!desktop && !('__FOLIO_TEST__' in window))
    return Promise.reject({
      code: 'DesktopRequired',
      message: 'Open the Folio desktop app to use your local expense workspace.',
    });
  return invoke<T>(command, args);
}
export const api = {
  expenses: () => call<Expense[]>('list_expenses'),
  expense: (id: string) => call<Expense>('get_expense', { id }),
  createExpense: () => call<Expense>('create_expense'),
  editExpense: (edit: ExpenseEdit) => call<Expense>('edit_expense', { edit }),
  queueExtraction: (id: string) => call<void>('queue_extraction', { id }),
  deleteExpenses: (ids: string[]) => call<number>('delete_expenses', { ids }),
  convertPending: () => call<number>('convert_pending'),
  importReceipts: (paths: string[]) => call<ImportOutcome[]>('import_receipts', { paths }),
  receipt: (id: string) => call<ReceiptContent>('read_receipt', { id }),
  thumbnail: (id: string) => call<string | null>('read_thumbnail', { id }),
  claims: () => call<Claim[]>('list_claims'),
  claim: (id: string) => call<ClaimDetail>('get_claim', { id }),
  createClaim: (title: string, currency: string) =>
    call<Claim>('create_claim', { title, currency }),
  editClaim: (id: string, version: number, title: string, description: string) =>
    call<Claim>('edit_claim', { id, version, title, description }),
  setClaimExpense: (claimId: string, expenseId: string, add: boolean) =>
    call<ClaimDetail>('set_claim_expense', { claimId, expenseId, add }),
  addClaimExpenses: (claimId: string, expenseIds: string[]) =>
    call<ClaimDetail>('add_claim_expenses', { claimId, expenseIds }),
  transitionClaim: (id: string, status: ClaimStatus) =>
    call<Claim>('transition_claim', { id, status }),
  requestPdf: (id: string) => call<string>('request_pdf', { id }),
  settings: () => call<Settings>('get_settings'),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', { settings }),
  info: () => call<AppInfo>('app_info'),
  setCredential: (key: string) => call<void>('set_credential', { key }),
  deleteCredential: () => call<void>('delete_credential'),
  jobs: () => call<Job[]>('list_jobs'),
  takeJob: () => call<JobLease | null>('take_job'),
  heartbeat: (id: string, token: string) => call<void>('heartbeat', { id, token }),
  failJob: (id: string, token: string, message: string) =>
    call<void>('fail_job', { id, token, message }),
  retryJob: (id: string) => call<void>('retry_job', { id }),
  onlineAvailable: () => call<boolean>('online_available'),
  tryOnline: (id: string, token: string, images: string[]) =>
    call<boolean>('try_online', { id, token, images }),
  completeOcr: (id: string, token: string, rawText: string, ocrConfidence: number | null) =>
    call<void>('complete_ocr', { id, token, rawText, ocrConfidence }),
  completeThumbnail: (id: string, token: string, base64: string) =>
    call<void>('complete_thumbnail', { id, token, base64 }),
  exportSnapshot: (id: string, token: string) =>
    call<ExportSnapshot>('export_snapshot', { id, token }),
  completePdf: (id: string, token: string, base64: string) =>
    call<string>('complete_pdf', { id, token, base64 }),
  openExport: (id: string) => call<void>('open_export', { id }),
  exportClaimCsv: (id: string) => call<CsvExport>('export_claim_csv', { id }),
  exportExpensesCsv: (ids: string[]) => call<CsvExport>('export_expenses_csv', { ids }),
  openCsvExport: (id: string) => call<void>('open_csv_export', { id }),
  exportClaimXlsx: (id: string) => call<CsvExport>('export_claim_xlsx', { id }),
  exportExpensesXlsx: (ids: string[]) => call<CsvExport>('export_expenses_xlsx', { ids }),
  openXlsxExport: (id: string) => call<void>('open_xlsx_export', { id }),
  // Finance ledger (docs/DECISIONS.md). `today` is the device's local date
  // (YYYY-MM-DD); periods are YYYY-MM.
  workspaces: () => call<Workspace[]>('list_workspaces'),
  saveWorkspace: (input: WorkspaceInput) => call<Workspace>('save_workspace', { input }),
  categories: (workspaceId: string) => call<Category[]>('list_categories', { workspaceId }),
  saveCategory: (input: CategoryInput) => call<Category>('save_category', { input }),
  deleteCategory: (id: string) => call<void>('delete_category', { id }),
  accounts: (workspaceId: string) => call<Account[]>('list_accounts', { workspaceId }),
  saveAccount: (input: AccountInput) => call<Account>('save_account', { input }),
  deleteAccount: (id: string) => call<void>('delete_account', { id }),
  transactions: (filter: TransactionFilter) =>
    call<TransactionPage>('list_transactions', { filter }),
  transaction: (id: string) => call<Transaction>('get_transaction', { id }),
  saveTransaction: (input: TransactionInput) => call<Transaction>('save_transaction', { input }),
  deleteTransactions: (ids: string[]) => call<number>('delete_transactions', { ids }),
  recordExpense: (input: RecordExpenseInput) => call<Transaction>('record_expense', { input }),
  budgets: (workspaceId: string, period: string) =>
    call<BudgetProgress[]>('list_budgets', { workspaceId, period }),
  saveBudget: (input: BudgetInput) => call<Budget>('save_budget', { input }),
  deleteBudget: (id: string) => call<void>('delete_budget', { id }),
  bills: (workspaceId: string) => call<Bill[]>('list_bills', { workspaceId }),
  bill: (id: string, today: string) => call<BillDetail>('get_bill', { id, today }),
  saveBill: (input: BillInput) => call<Bill>('save_bill', { input }),
  deleteBill: (id: string) => call<void>('delete_bill', { id }),
  billOccurrences: (workspaceId: string, today: string, horizonDays: number) =>
    call<BillOccurrence[]>('list_bill_occurrences', { workspaceId, today, horizonDays }),
  payBill: (input: BillPaymentInput) => call<Transaction>('pay_bill', { input }),
  monthlyReport: (workspaceId: string, period: string) =>
    call<MonthlyReport>('report_monthly', { workspaceId, period }),
  overview: (workspaceId: string, today: string) =>
    call<Overview>('report_overview', { workspaceId, today }),
  exportTransactionsCsv: (filter: TransactionFilter) =>
    call<CsvExport>('export_transactions_csv', { filter }),
  reminders: (today: string, horizonDays: number) =>
    call<Reminder[]>('list_reminders', { today, horizonDays }),
  ackReminder: (id: string) => call<void>('ack_reminder', { id }),
  parseQuickAdd: (workspaceId: string, text: string, today: string) =>
    call<QuickAddDraft>('parse_quick_add', { workspaceId, text, today }),
  suggestions: (workspaceId: string) =>
    call<TransactionSuggestion[]>('list_suggestions', { workspaceId }),
  acceptSuggestion: (id: string, input: TransactionInput) =>
    call<Transaction>('accept_suggestion', { id, input }),
  dismissSuggestion: (id: string) => call<void>('dismiss_suggestion', { id }),
};
/** The device's local calendar date, YYYY-MM-DD (D4: never the UTC date). */
export function localToday(now = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
