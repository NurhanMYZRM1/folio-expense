import { invoke, isTauri } from '@tauri-apps/api/core';
import type {
  AppInfo,
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
  tryOnline: (id: string, token: string, images: string[]) =>
    call<boolean>('try_online', { id, token, images }),
  completeOcr: (id: string, token: string, rawText: string) =>
    call<void>('complete_ocr', { id, token, rawText }),
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
};
