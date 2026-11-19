import * as Notifications from "expo-notifications";
import { File } from "expo-file-system";
import { Platform } from "react-native";
import type { Reminder } from "@folio/bindings/generated";
import { call, dataRoot } from "./folio";

const PREFIX = "folio:";
const CHANNEL = "folio-finance";
let running: Promise<void> | null = null;
let again = false;
function today() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
function fireDate(r: Reminder) {
  const [y, m, d] = r.fireOn.split("-").map(Number);
  return new Date(y, m - 1, d, 9);
}
export async function requestReminders(): Promise<boolean> {
  if (Platform.OS === "android") await Notifications.setNotificationChannelAsync(CHANNEL, { name: "Bills and budgets", importance: Notifications.AndroidImportance.DEFAULT });
  const permission = await Notifications.requestPermissionsAsync();
  if (permission.granted) await syncReminders();
  return permission.granted;
}
async function reschedule() {
  const permission = await Notifications.getPermissionsAsync();
  if (Platform.OS === "android") await Notifications.setNotificationChannelAsync(CHANNEL, { name: "Bills and budgets", importance: Notifications.AndroidImportance.DEFAULT });
  const scheduled = (await Notifications.getAllScheduledNotificationsAsync()).filter(r => r.identifier.startsWith(PREFIX));
  if (!permission.granted) {
    for (const r of scheduled) await Notifications.cancelScheduledNotificationAsync(r.identifier);
    return;
  }
  const reminders = await call<Reminder[]>("list_reminders", { today: today(), horizonDays: 30 });
  // Retain device-local schedule receipts so overdue bills do not alert again
  // each time the app opens. No amounts, titles or financial content is stored.
  const file = new File(`file://${encodeURI(dataRoot())}/cache/reminders.json`);
  let previous: Record<string, number> = {};
  if (file.exists) {
    try { previous = JSON.parse(await file.text()) as Record<string, number>; } catch { /* rebuild an unreadable scheduler cache */ }
  }
  const now = Date.now();
  const next: Record<string, number> = {};
  const wanted = new Set(reminders.map(r => PREFIX + r.id));
  for (const r of scheduled) if (!wanted.has(r.identifier)) await Notifications.cancelScheduledNotificationAsync(r.identifier);
  // iOS allows only a limited number of pending notifications. Earliest first.
  for (const r of [...reminders].sort((a, b) => a.fireOn.localeCompare(b.fireOn)).slice(0, 60)) {
    const identifier = PREFIX + r.id;
    const fire = fireDate(r).getTime();
    next[identifier] = fire;
    if (fire <= now && previous[identifier] != null && previous[identifier] <= now) continue;
    await Notifications.cancelScheduledNotificationAsync(identifier);
    await Notifications.scheduleNotificationAsync({
      identifier,
      content: { title: r.title, body: r.body, data: { route: r.kind === "bill" ? "/finance/bills" : "/finance/budgets" } },
      trigger: fire > now ? { type: Notifications.SchedulableTriggerInputTypes.DATE, date: new Date(fire), channelId: CHANNEL } : null,
    });
    // Persist before acknowledging core budget alerts. Stable identifiers also
    // avoid duplicates if the process is interrupted while rescheduling.
    file.write(JSON.stringify({ ...previous, ...next }));
    if (r.kind === "budget") await call("ack_reminder", { id: r.id });
  }
  // Keep acknowledged budget receipts for the current month, plus current bills.
  for (const [id, fire] of Object.entries(previous)) if (fire > now - 32 * 86400000 && !(id in next)) next[id] = fire;
  file.write(JSON.stringify(next));
}
export function syncReminders(): Promise<void> {
  if (running) { again = true; return running; }
  running = (async () => {
    do { again = false; await reschedule(); } while (again);
  })().finally(() => { running = null; });
  return running;
}
