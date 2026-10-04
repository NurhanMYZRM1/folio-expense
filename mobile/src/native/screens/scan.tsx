// Native redesign of ../src/features/receipts/ImportReceipts.tsx: capture with
// the camera, photo library or Files, then follow each receipt as Folio reads it.
import * as Haptics from "expo-haptics";
import { router } from "expo-router";
import { SymbolView, type SFSymbol } from "expo-symbols";
import { useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, useColorScheme, View } from "react-native";
import type { Expense, ImportOutcome } from "@folio/bindings/generated";
import { call, errorText } from "~/core/folio";
import { pickReceipts, type ReceiptSource } from "~/core/pickers";
import { ExpenseRow } from "../expense-row";
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
        { backgroundColor: primary ? p.accent : p.surface, borderColor: p.border, opacity: disabled ? 0.5 : pressed ? 0.85 : 1 },
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
  const [busy, setBusy] = useState(false);
  const [problems, setProblems] = useState<ImportOutcome[]>([]);

  const importFrom = async (source: ReceiptSource) => {
    try {
      const paths = await pickReceipts(source);
      if (!paths.length) return;
      setBusy(true);
      const outcomes = await call<ImportOutcome[]>("import_receipts", { paths });
      const saved = outcomes.filter((o) => o.expenseId).length;
      setProblems(outcomes.filter((o) => o.error));
      if (saved) {
        void Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
        notify(`${saved} ${saved === 1 ? "receipt saved" : "receipts saved"} on this device. Reading has started.`);
      }
    } catch (e) {
      notify(errorText(e), true);
    } finally {
      setBusy(false);
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
          <Action icon="doc" title="Files" subtitle="PDF, JPEG or PNG" onPress={() => void importFrom("files")} p={p} disabled={busy} />
        </View>
      </View>
      {busy && (
        <View style={s.busy}>
          <ActivityIndicator color={p.accent} />
          <Text style={{ color: p.muted }}>Saving receipts…</Text>
        </View>
      )}
      {problems.map((o, i) => (
        <Pressable
          key={`${o.filename}-${i}`}
          disabled={!o.error?.existingExpenseId}
          onPress={() => o.error?.existingExpenseId && router.push(`/expenses/${o.error.existingExpenseId}`)}
          style={[s.problem, { backgroundColor: p.warningBg }]}
        >
          <Text style={[s.problemTitle, { color: p.warning }]}>{o.filename}</Text>
          <Text style={{ color: p.text }}>{o.error?.message}</Text>
          {o.error?.existingExpenseId && <Text style={{ color: p.accent, fontWeight: "600" }}>Open existing expense</Text>}
        </Pressable>
      ))}
      {recent.length > 0 && (
        <View style={[s.card, { backgroundColor: p.surface, borderColor: p.border }]}>
          <Text style={[s.cardTitle, { color: p.text }]}>Recent receipts</Text>
          {recent.map((e, i) => (
            <View key={e.id} style={i > 0 ? { borderTopWidth: StyleSheet.hairlineWidth, borderTopColor: p.border } : undefined}>
              <ExpenseRow expense={e} />
            </View>
          ))}
        </View>
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
  action: { flexDirection: "row", alignItems: "center", gap: 14, padding: 16, borderRadius: 16, borderCurve: "continuous", borderWidth: StyleSheet.hairlineWidth, minHeight: 72 },
  actionTitle: { fontSize: 17, fontWeight: "600" },
  actionSub: { fontSize: 13, marginTop: 2 },
  pair: { flexDirection: "row", gap: 12 },
  busy: { flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 8 },
  problem: { padding: 14, borderRadius: 12, gap: 4 },
  problemTitle: { fontWeight: "600" },
  card: { borderRadius: 16, borderCurve: "continuous", borderWidth: StyleSheet.hairlineWidth, overflow: "hidden" },
  cardTitle: { fontSize: 15, fontWeight: "600", paddingHorizontal: 16, paddingTop: 14, paddingBottom: 4 },
  privacy: { flexDirection: "row", gap: 8, alignItems: "flex-start", paddingHorizontal: 4, paddingTop: 4 },
  privacyText: { flex: 1, fontSize: 13, lineHeight: 18 },
});
