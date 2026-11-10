// What a shared Folio screen gets from the native side when it runs in a DOM
// component: core calls, pickers, PDF pages, sharing and navigation.
import { Asset } from "expo-asset";
import { Directory, File, Paths } from "expo-file-system";
import { router, useLocalSearchParams, useNavigation, type Href } from "expo-router";
import * as Sharing from "expo-sharing";
import { useCallback, useMemo } from "react";
import { ActionSheetIOS, Alert, Platform } from "react-native";
import FolioCore from "../../modules/folio-core";
import { requestReminders } from "~/core/reminders";
import { dataRoot, invokeRaw, useDataVersion } from "~/core/folio";
import { pickReceipts, type ReceiptSource } from "~/core/pickers";
import type { FolioHostProps, NavigateMode } from "~/dom/host";

const TAB_ROOTS = new Set(["/", "/expenses", "/import", "/claims", "/settings"]);

// Exports are stored under their IDs; share them under the names the desktop gives them.
const claimNumbers = new Map<string, string>();
const exportNames = new Map<string, string>();

async function share(path: string, name: string | undefined) {
  if (!(await Sharing.isAvailableAsync())) return;
  let uri = `file://${encodeURI(path)}`;
  if (name) {
    const dir = new Directory(Paths.cache, "folio-share");
    dir.create({ intermediates: true, idempotent: true });
    const copy = new File(dir, name.replace(/[/\\]/g, "_"));
    new File(uri).copySync(copy, { overwrite: true });
    uri = copy.uri;
  }
  await Sharing.shareAsync(uri);
}

function remember(command: string, args: string, response: string) {
  const result = (JSON.parse(response) as { result?: unknown }).result;
  if (result == null) return;
  if (command === "get_claim") {
    const claim = (result as { claim: { id: string; claimNumber: string } }).claim;
    claimNumbers.set(claim.id, claim.claimNumber);
  } else if (
    command === "save_invoice_pdf" ||
    command === "export_all_data" ||
    command === "export_transactions_csv" ||
    command === "export_claim_csv" ||
    command === "export_expenses_csv" ||
    command === "export_claim_xlsx" ||
    command === "export_expenses_xlsx"
  ) {
    const csv = result as { id: string; fileName: string };
    exportNames.set(csv.id, csv.fileName);
  } else if (command === "request_pdf" && typeof result === "string") {
    const claimId = (JSON.parse(args) as { id?: string }).id ?? "";
    exportNames.set(result, `Folio-${claimNumbers.get(claimId) ?? "claim"}.pdf`);
  }
}

/** Generated reports, CSVs and Excel workbooks open in the share sheet (Files, Mail, Print…). */
async function invoke(command: string, args: string): Promise<string> {
  if (command === "read_platform_info") return JSON.stringify({ result: { platform: Platform.OS, paymentAccess: Platform.OS === "android" && FolioCore.paymentNotificationAccess() } });
  if (command === "request_payment_access") {
    if (Platform.OS !== "android") return JSON.stringify({ error: { code: "Unsupported", message: "Payment notification access is Android only." } });
    await FolioCore.requestPaymentNotificationAccess();
    return JSON.stringify({ result: null });
  }
  if (command === "request_reminders") return JSON.stringify({ result: await requestReminders() });
  if (command === "transcribe_voice") {
    try { return JSON.stringify({ result: await FolioCore.transcribeVoice() }); }
    catch (e) { return JSON.stringify({ error: { code: "SpeechUnavailable", message: e instanceof Error ? e.message : String(e) } }); }
  }
  if (command === "cancel_transcription") { FolioCore.cancelTranscription(); return JSON.stringify({ result: null }); }
  if (command === "read_invoice_font") {
    // Metro bundles the same font used by the native claim PDF generator.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const asset = Asset.fromModule(require("@fontsource/noto-sans/files/noto-sans-latin-400-normal.woff"));
    await asset.downloadAsync();
    return JSON.stringify({ result: new File(asset.localUri ?? asset.uri).base64Sync() });
  }
  const response = await invokeRaw(command, args);
  if (response.startsWith('{"error"')) return response;
  if (command === "open_invoice_pdf" || command === "open_export" || command === "open_csv_export" || command === "open_xlsx_export" || command === "open_data_export") {
    const path = (JSON.parse(response) as { result?: string }).result;
    const id = (JSON.parse(args) as { id?: string }).id ?? "";
    if (path) await share(path, exportNames.get(id) ?? (command === "open_invoice_pdf" || command === "open_export" ? "Folio claim report.pdf" : undefined));
    return JSON.stringify({ result: null });
  }
  remember(command, args, response);
  return response;
}

function chooseSource(): Promise<ReceiptSource | null> {
  const labels = ["Take Photo", "Choose from Library", "Choose Files", "Cancel"];
  const sources: ReceiptSource[] = ["camera", "library", "files"];
  return new Promise((resolve) => {
    if (Platform.OS === "ios") {
      ActionSheetIOS.showActionSheetWithOptions({ options: labels, cancelButtonIndex: 3 }, (i) => resolve(sources[i] ?? null));
    } else {
      Alert.alert("Add receipts", undefined, [
        ...sources.map((s, i) => ({ text: labels[i], onPress: () => resolve(s) })),
        { text: "Cancel", style: "cancel" as const, onPress: () => resolve(null) },
      ]);
    }
  });
}

async function pick(options: string): Promise<string> {
  const o = JSON.parse(options) as { directory?: boolean };
  if (o.directory) {
    Alert.alert("Not available on this device", "Exports are shared from the claim instead of being copied to a folder.");
    return "null";
  }
  const source = await chooseSource();
  if (!source) return "null";
  const { paths, failed } = await pickReceipts(source);
  // The shared import screen only takes paths, so name the skipped photos here.
  if (failed.length) {
    Alert.alert(
      failed.length === 1 ? "1 photo was skipped" : `${failed.length} photos were skipped`,
      failed.map((f) => `${f.filename}: ${f.error?.message}`).join("\n\n"),
    );
  }
  return JSON.stringify(paths.length ? paths : null);
}

async function renderPdf(relativePath: string): Promise<string> {
  const pages = await FolioCore.renderPdfPages(`${dataRoot()}/${relativePath}`, 30, 1600);
  return JSON.stringify(pages.map((p) => `data:image/jpeg;base64,${new File(`file://${encodeURI(p)}`).base64Sync()}`));
}

/**
 * `template` is the screen's own path in the desktop router ("/expenses/:id").
 * Tabs stay mounted while hidden, so the global pathname can't be used.
 */
export function useFolioDomProps(template: string): FolioHostProps {
  const params = useLocalSearchParams();
  const navigation = useNavigation();
  const dataVersion = useDataVersion();

  const path = useMemo(() => {
    const used = new Set<string>();
    const pathname = template.replace(/:(\w+)/g, (_m, key: string) => {
      used.add(key);
      return encodeURIComponent(String(params[key] ?? ""));
    });
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(params)) {
      if (used.has(k)) continue;
      for (const one of Array.isArray(v) ? v : [v]) if (one != null) q.append(k, String(one));
    }
    const query = q.toString();
    return query ? `${pathname}?${query}` : pathname;
  }, [template, params]);

  const navigate = useCallback(async (to: string, _mode: NavigateMode) => {
    const target = to as Href;
    if (TAB_ROOTS.has(to.split("?")[0]) && router.canGoBack()) router.dismissTo(target);
    else router.push(target);
  }, []);

  const setTitle = useCallback(async (title: string) => navigation.setOptions({ title }), [navigation]);

  return {
    invoke,
    pick,
    renderPdf,
    navigate,
    setTitle,
    syncTitle: !TAB_ROOTS.has(template),
    path,
    dataVersion,
    dom: { style: { flex: 1 } },
  };
}
