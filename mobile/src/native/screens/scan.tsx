// Native redesign of ../src/features/receipts/ImportReceipts.tsx: capture with
// the camera, photo library or Files, then follow each receipt as Folio reads it.
import * as Haptics from "expo-haptics";
import { router } from "expo-router";
import { SymbolView, type SFSymbol } from "expo-symbols";
import { useState } from "react";
import { Pressable, ScrollView, StyleSheet, Text, useColorScheme, View } from "react-native";
import Animated from "react-native-reanimated";
import type { Expense, ImportOutcome } from "@folio/bindings/generated";
import { importSequentially } from "@folio/lib/importQueue";
import { call, errorText } from "~/core/folio";
import { pickReceipts, type ReceiptSource } from "~/core/pickers";
import { ExpenseRow } from "../expense-row";
import { listEntering, panelEntering, ProgressBar, quickExit, Reveal, settle } from "../motion";
import { notify } from "../status";
import { palette, type Palette } from "../theme";
import { useFolio } from "../use-folio";

function Action({ icon, title, subtitle, primary, onPress, p, disabled }: { icon: SFSymbol; title: string; subtitle: string; primary?: boolean; onPress: () => void; p: Palette; disabled?: boolean }) {
  return (
    <Pressable
      disabled={disabled}
      onPress={onPress}
      accessibilityRole="button"
      style={({ pressed }) => [
        s.action,
        {
          backgroundColor: primary ? p.accent : p.surface,
          boxShadow: primary ? p.shadowPrimary : p.shadow,
          opacity: disabled ? 0.5 : 1,
          // Press feedback: a slight scale-down, like the desktop's buttons.
          transform: [{ scale: pressed ? 0.98 : 1 }],
        },
      ]}
    >
      <SymbolView name={icon} size={24} tintColor={primary ? p.bg : p.accent} />
      <View style={{ flex: 1 }}>
        <Text style={[s.actionTitle, { color: primary ? p.bg : p.text }]}>{title}</Text>
        <Text style={[s.actionSub, { color: primary ? p.bg : p.muted, opacity: primary ? 0.85 : 1 }]}>{subtitle}</Text>
      </View>
    </Pressable>
  );
}

export default function ScanScreen() {
  const p = palette[useColorScheme() === "dark" ? "dark" : "light"];
  const expenses = useFolio<Expense[]>("list_expenses");
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const busy = progress !== null;
  const [problems, setProblems] = useState<ImportOutcome[]>([]);

  const importFrom = async (source: ReceiptSource) => {
    try {
      const paths = await pickReceipts(source);
      if (!paths.length) return;
      setProblems([]);
      // One receipt per call, so the bar shows real progress (shared with the desktop).
      const outcomes = await importSequentially(
        paths,
        (batch) => call<ImportOutcome[]>("import_receipts", { paths: batch }),
        (done, total) => setProgress({ done, total }),
      );
      const saved = outcomes.filter((o) => o.expenseId).length;
      setProblems(outcomes.filter((o) => o.error));
      if (saved) {
        void Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
        notify(`${saved} ${saved === 1 ? "receipt saved" : "receipts saved"} on this device. Reading has started.`);
      }
    } catch (e) {
      notify(errorText(e), true);
    } finally {
      setProgress(null);
    }
  };

  const recent = [...(expenses.data ?? [])]
    .filter((e) => e.receiptId)
    .sort((a, b) => (a.createdAt < b.createdAt ? 1 : -1))
    .slice(0, 8);

  return (
    <ScrollView style={{ flex: 1, backgroundColor: p.bg }} contentInsetAdjustmentBehavior="automatic" contentContainerStyle={s.content}>
      <Text style={[s.lead, { color: p.muted }]}>Folio keeps the original, reads the receipt on this device and suggests the details.</Text>
      <Action icon="camera.fill" title="Take a photo" subtitle="Lay the receipt flat in good light" primary onPress={() => void importFrom("camera")} p={p} disabled={busy} />
      <View style={s.pair}>
        <View style={{ flex: 1 }}>
          <Action icon="photo.on.rectangle" title="Photos" subtitle="From your library" onPress={() => void importFrom("library")} p={p} disabled={busy} />
        </View>
        <View style={{ flex: 1 }}>
          <Action icon="doc" title="Files" subtitle="PDF, JPEG, PNG or HEIC" onPress={() => void importFrom("files")} p={p} disabled={busy} />
        </View>
      </View>
      {progress && (
        <Animated.View entering={panelEntering} exiting={quickExit} style={s.busy} accessibilityLiveRegion="polite">
          <Text style={[s.busyText, { color: p.muted }]}>
            Saving receipt {Math.min(progress.done + 1, progress.total)} of {progress.total}…
          </Text>
          <ProgressBar value={progress.total ? progress.done / progress.total : 0} color={p.accent} track={p.border} />
        </Animated.View>
      )}
      {problems.map((o, i) => (
        <Reveal key={`${o.filename}-${i}`} index={i}>
          <Pressable
            disabled={!o.error?.existingExpenseId}
            onPress={() => o.error?.existingExpenseId && router.push(`/expenses/${o.error.existingExpenseId}`)}
            style={[s.problem, { backgroundColor: p.warningBg }]}
          >
            <Text style={[s.problemTitle, { color: p.warning }]}>{o.filename}</Text>
            <Text style={{ color: p.text }}>{o.error?.message}</Text>
            {o.error?.existingExpenseId && <Text style={{ color: p.accent, fontWeight: "600" }}>Open existing expense</Text>}
          </Pressable>
        </Reveal>
      ))}
      {recent.length > 0 && (
        // The outer view casts the shadow; the inner one clips rows to the rounded corners.
        <Animated.View entering={panelEntering} layout={settle} style={[s.card, { backgroundColor: p.surface, boxShadow: p.shadow }]}>
          <View style={s.cardClip}>
            <Text style={[s.cardTitle, { color: p.text }]}>Recent receipts</Text>
            {recent.map((e, i) => (
              // New receipts slide in at the top; the rest glide down to make room.
              <Animated.View
                key={e.id}
                entering={listEntering(i)}
                exiting={quickExit}
                layout={settle}
                style={i > 0 ? { borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: p.border } : undefined}
              >
                <ExpenseRow expense={e} />
              </Animated.View>
            ))}
          </View>
        </Animated.View>
      )}
      <View style={s.privacy}>
        <SymbolView name="lock.shield" size={16} tintColor={p.accent} />
        <Text style={[s.privacyText, { color: p.muted }]}>Receipts and extracted text stay on this device. No account or upload is needed.</Text>
      </View>
    </ScrollView>
  );
}

const s = StyleSheet.create({
  content: { padding: 16, paddingBottom: 120, gap: 12 },
  lead: { fontSize: 15, lineHeight: 21 },
  action: { flexDirection: "row", alignItems: "center", gap: 14, padding: 16, borderRadius: 16, borderCurve: "continuous", minHeight: 72 },
  actionTitle: { fontSize: 17, fontWeight: "600" },
  actionSub: { fontSize: 13, marginTop: 2 },
  pair: { flexDirection: "row", gap: 12 },
  busy: { gap: 8, paddingVertical: 8 },
  busyText: { fontSize: 13, fontVariant: ["tabular-nums"] },
  problem: { padding: 14, borderRadius: 12, gap: 4 },
  problemTitle: { fontWeight: "600" },
  card: { borderRadius: 16, borderCurve: "continuous" },
  cardClip: { borderRadius: 16, borderCurve: "continuous", overflow: "hidden" },
  cardTitle: { fontSize: 15, fontWeight: "600", paddingHorizontal: 16, paddingTop: 14, paddingBottom: 4 },
  privacy: { flexDirection: "row", gap: 8, alignItems: "flex-start", paddingHorizontal: 4, paddingTop: 4 },
  privacyText: { flex: 1, fontSize: 13, lineHeight: 18 },
});
