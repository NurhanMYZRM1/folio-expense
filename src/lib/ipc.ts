import { invoke, isTauri } from '@tauri-apps/api/core';
import type {
  ReceiptItemsReview,
  ReceiptItemsInput,
  TaxRules,
  IncomeTaxInput,
  IncomeTaxEstimate,
  SstInput,
  SstCalculation,
  EinvoiceValidation,
  BusinessProfile,
  CustomerInput,
  Customer,
  InventoryItemInput,
  InventoryItem,
  StockAdjustmentInput,
  StockMovement,
  InvoiceInput,
  Invoice,
  InvoicePaymentInput,
  BusinessReport,
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
  DataExport,
  Expense,
  ExpenseEdit,
  ExportSnapshot,
  ImportOutcome,
  Job,
  JobLease,
  ReceiptContent,
  Settings,
} from '../bindings/generated';
export type CloudStatus = {
  signedIn: boolean;
  /** A cloud account owns these records, whether or not it is signed in now. */
  linked: boolean;
  email: string | null;
  providers: string[];
  pending: number;
  conflicts: number;
  lastSync: string | null;
  error: string | null;
  state:
    'local' | 'signedOut' | 'signInAgain' | 'offline' | 'needsAttention' | 'pending' | 'upToDate';
  failure: string | null;
  retryAt: string | null;
  requestId: string | null;
};
/** What a sign-in attempt led to. `chooseData` waits for `cloudAdopt`. */
export type CloudOutcome =
  | { outcome: 'signedIn' | 'switched' | 'linked' | 'cancelled'; status: CloudStatus }
  | { outcome: 'chooseData'; email: string | null };
export type CloudProvider = 'google' | 'apple';
export type CloudDiagnosis = {
  environment: string;
  project: string;
  keyKind: string;
  callback: string;
  reachable: string;
  authenticated: string;
  pendingFiles: number;
  status: CloudStatus;
};
export type CloudConflict = {
  id: string;
  entity: string;
  local: unknown;
  remote: unknown;
};
export const desktop = isTauri();
function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!desktop && !('__FOLIO_TEST__' in window))
    return Promise.reject({
      code: 'DesktopRequired',
      message: 'Open the Folio desktop app to use your local expense workspace.',
    });
  const result = invoke<T>(command, args);
  // Lets sync follow an edit promptly without each screen asking for it.
  if (!QUIET.test(command))
    void result.then(
      () => window.dispatchEvent(new Event('folio:changed')),
      () => undefined,
    );
  return result;
}
/** Commands that never change records, so need no sync afterwards. */
const QUIET =
  /^(list|get|read|report|parse|cloud|open|workspace)_|^(app_info|take_job|heartbeat|online_available|export_snapshot)$/;
export const api = {
  cloudStatus: () => call<CloudStatus>('cloud_status'),
  cloudSignIn: (email: string, password: string) =>
    call<CloudOutcome>('cloud_sign_in', { email, password }),
  cloudRequestCode: (email: string) => call<CloudStatus>('cloud_request_code', { email }),
  cloudVerifyCode: (email: string, code: string) =>
    call<CloudOutcome>('cloud_verify_code', { email, code }),
  /**
   * Opens Google or Apple in the system browser. The phone finishes inside this
   * call; the desktop returns nothing and finishes when the browser comes back.
   */
  cloudOauthStart: (provider: CloudProvider, link: boolean) =>
    call<CloudOutcome | null>('cloud_oauth_start', { provider, link }),
  /** Google and Apple as switched on for the project; null when it cannot be reached. */
  cloudProviders: () => call<CloudProvider[] | null>('cloud_providers'),
  cloudOauthFinish: (url: string) => call<CloudOutcome>('cloud_oauth_finish', { url }),
  cloudOauthCancel: () => call<null>('cloud_oauth_cancel'),
  cloudAdopt: (choice: 'join' | 'separate' | 'cancel') =>
    call<CloudOutcome>('cloud_adopt', { choice }),
  cloudDiagnose: () => call<CloudDiagnosis>('cloud_diagnose'),
  cloudSignOut: () => call<CloudStatus>('cloud_sign_out'),
  cloudSync: (force = false) => call<CloudStatus>('cloud_sync', { force }),
  cloudConflicts: () => call<CloudConflict[]>('cloud_conflicts'),
  cloudResolveConflict: (id: string, choice: 'local' | 'remote') =>
    call<CloudStatus>('cloud_resolve_conflict', { id, choice }),
  exportAllData: () => call<DataExport>('export_all_data'),
  openDataExport: (id: string) => call<void>('open_data_export', { id }),
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
  saveInvoicePdf: (workspaceId: string, invoiceId: string, version: number, base64: string) =>
    call<CsvExport>('save_invoice_pdf', { workspaceId, invoiceId, version, base64 }),
  openInvoicePdf: (id: string) => call<void>('open_invoice_pdf', { id }),
  receiptItems: (expenseId: string) => call<ReceiptItemsReview>('get_receipt_items', { expenseId }),
  saveReceiptItems: (input: ReceiptItemsInput) =>
    call<ReceiptItemsReview>('save_receipt_items', { input }),
  businessProfile: (workspaceId: string) =>
    call<BusinessProfile | null>('get_business_profile', { workspaceId }),
  saveBusinessProfile: (input: BusinessProfile) =>
    call<BusinessProfile>('save_business_profile', { input }),
  customers: (workspaceId: string, includeArchived = false) =>
    call<Customer[]>('list_customers', { workspaceId, includeArchived }),
  saveCustomer: (input: CustomerInput) => call<Customer>('save_customer', { input }),
  deleteCustomer: (id: string) => call<void>('delete_customer', { id }),
  deleteInventoryItem: (id: string) => call<void>('delete_inventory_item', { id }),
  inventoryItems: (workspaceId: string, includeArchived = false) =>
    call<InventoryItem[]>('list_inventory_items', { workspaceId, includeArchived }),
  saveInventoryItem: (input: InventoryItemInput) =>
    call<InventoryItem>('save_inventory_item', { input }),
  adjustStock: (input: StockAdjustmentInput) => call<InventoryItem>('adjust_stock', { input }),
  stockMovements: (workspaceId: string, itemId: string) =>
    call<StockMovement[]>('list_stock_movements', { workspaceId, itemId }),
  invoices: (workspaceId: string) => call<Invoice[]>('list_invoices', { workspaceId }),
  invoice: (workspaceId: string, id: string) => call<Invoice>('get_invoice', { workspaceId, id }),
  saveInvoice: (input: InvoiceInput) => call<Invoice>('save_invoice', { input }),
  issueInvoice: (workspaceId: string, id: string, version: number) =>
    call<Invoice>('issue_invoice', { workspaceId, id, version }),
  voidInvoice: (workspaceId: string, id: string, version: number) =>
    call<Invoice>('void_invoice', { workspaceId, id, version }),
  recordInvoicePayment: (input: InvoicePaymentInput) =>
    call<Invoice>('record_invoice_payment', { input }),
  taxRules: (assessmentYear: number) => call<TaxRules>('get_tax_rules', { assessmentYear }),
  estimateIncomeTax: (input: IncomeTaxInput) =>
    call<IncomeTaxEstimate>('estimate_income_tax', { input }),
  calculateSst: (input: SstInput) => call<SstCalculation>('calculate_sst', { input }),
  validateEinvoice: (workspaceId: string, invoiceId: string) =>
    call<EinvoiceValidation>('validate_einvoice', { workspaceId, invoiceId }),
  businessReport: (workspaceId: string, today: string) =>
    call<BusinessReport>('report_business', { workspaceId, today }),
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
