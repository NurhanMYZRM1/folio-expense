// The Rust core (../src-tauri) through the FolioCore native module. Commands
// and arguments are exactly the desktop's Tauri commands (src/lib/ipc.ts).
import { useSyncExternalStore } from "react";
import FolioCore from "../../modules/folio-core";
import type { AppError } from "@folio/bindings/generated";

/** Commands that only read; anything else may change records. */
const READS = new Set([
  "list_expenses", "get_expense", "read_receipt", "read_thumbnail", "list_claims", "get_claim",
  "get_settings", "app_info", "list_jobs", "export_snapshot", "open_export", "open_csv_export",
]);

let opened: Promise<void> | null = null;
let version = 0;
const listeners = new Set<() => void>();

export const dataRoot = () => FolioCore.dataRoot();

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
  opened ??= FolioCore.open(dataRoot()).then((r) => {
    parse<null>(r);
  });
  return opened;
}

/** Polling that only changes records when it returns work. */
function changesRecords(command: string, response: string): boolean {
  if (READS.has(command) || response.startsWith('{"error"')) return false;
  if (command === "take_job" || command === "heartbeat") return false;
  if (command === "convert_pending") return response !== '{"result":0}';
  return true;
}

export async function invokeRaw(command: string, args: string): Promise<string> {
  await openCore();
  const response = await FolioCore.invoke(command, args);
  if (changesRecords(command, response)) changed();
  return response;
}

export async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return parse<T>(await invokeRaw(command, JSON.stringify(args)));
}

export function errorText(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
