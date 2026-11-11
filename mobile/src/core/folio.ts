// The Rust core (../src-tauri) through the FolioCore native module. Commands
// and arguments are exactly the desktop's Tauri commands (src/lib/ipc.ts).
import { useSyncExternalStore } from "react";
import { AppState } from "react-native";
import FolioCore from "../../modules/folio-core";
import type { AppError } from "@folio/bindings/generated";

/** Commands that only read; anything else may change records. */
const READS = new Set([
  "cloud_status", "cloud_conflicts", "cloud_request_code", "cloud_diagnose", "cloud_oauth_start",
  "cloud_oauth_cancel", "workspace_info",
  "list_expenses", "get_expense", "read_receipt", "read_thumbnail", "list_claims", "get_claim",
  "get_settings", "app_info", "list_jobs", "export_snapshot", "open_export", "open_csv_export",
  "open_xlsx_export", "open_data_export",
]);

const READ_ONLY_NAME = /^(list|get|read|report|parse)_/;

let opened: Promise<void> | null = null;
let version = 0;
const listeners = new Set<() => void>();
/** The person's own changes, which schedule an upload; never a sync's own result. */
const edits = new Set<() => void>();

/** Where the app keeps everything; each cloud account has its own folder inside. */
const baseRoot = () => FolioCore.dataRoot();
let activeRoot: string | null = null;
/** The data folder of the account that is active now. */
export const dataRoot = () => activeRoot ?? baseRoot();
/** Signing in can move the app to another account's records. */
const SIGN_IN = new Set(["cloud_sign_in", "cloud_verify_code", "cloud_oauth_finish", "cloud_adopt"]);
async function readActiveRoot() {
  activeRoot = parse<{ root: string }>(await FolioCore.invoke("workspace_info", "{}")).root;
}

function changed() {
  version++;
  for (const l of listeners) l();
}

export function useDataVersion(): number {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => version,
  );
}

export function currentVersion() {
  return version;
}

function parse<T>(json: string): T {
  const r = JSON.parse(json) as { result?: T; error?: AppError };
  if (r.error) throw r.error;
  return r.result as T;
}

export function openCore(): Promise<void> {
  opened ??= FolioCore.open(baseRoot()).then((r) => {
    parse<null>(r);
    return readActiveRoot();
  });
  return opened;
}

/** Polling that only changes records when it returns work. */
function changesRecords(command: string, response: string): boolean {
  // Core commands follow a read-only naming rule (docs/DECISIONS.md, D5).
  if (READS.has(command) || READ_ONLY_NAME.test(command) || response.startsWith('{"error"'))
    return false;
  if (command === "take_job" || command === "heartbeat") return false;
  if (command === "convert_pending") return response !== '{"result":0}';
  return true;
}

export async function invokeRaw(command: string, args: string): Promise<string> {
  await openCore();
  const response = await FolioCore.invoke(command, args);
  // File paths must follow the account before any screen reloads its records.
  if (SIGN_IN.has(command) && !response.startsWith('{"error"')) await readActiveRoot();
  if (changesRecords(command, response)) {
    changed();
    // A sync that leaves work pending (offline, a held conflict, a receipt still
    // being read) must not schedule another sync, or it would repeat every 2 s.
    if (!command.startsWith("cloud_")) edits.forEach((l) => l());
  }
  return response;
}

export async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return parse<T>(await invokeRaw(command, JSON.stringify(args)));
}

export function errorText(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

/** Native screens also sync when no shared DOM screen is mounted. */
export function startCloudSync(): () => void {
  let stopped = false;
  let running = false;
  let debounce: ReturnType<typeof setTimeout> | undefined;
  const run = async (onlyIfPending = false) => {
    if (stopped || running || AppState.currentState !== "active") return;
    running = true;
    try {
      const status = await call<{ signedIn: boolean; pending: number }>("cloud_status");
      if (!stopped && status.signedIn && (!onlyIfPending || status.pending > 0)) await call("cloud_sync");
    } catch {
      // The native core persists sync errors; Settings shows them on demand.
    } finally {
      running = false;
    }
  };
  void run();
  const timer = setInterval(() => void run(), 20_000);
  // Catch up as soon as the app returns; iOS does not let it sync while suspended.
  const listener = AppState.addEventListener("change", state => {
    if (state === "active") void run();
  });
  // Upload shortly after an edit instead of waiting for the next poll.
  const edited = () => {
    clearTimeout(debounce);
    debounce = setTimeout(() => void run(true), 2000);
  };
  edits.add(edited);
  return () => {
    stopped = true;
    clearInterval(timer);
    clearTimeout(debounce);
    edits.delete(edited);
    listener.remove();
  };
}
