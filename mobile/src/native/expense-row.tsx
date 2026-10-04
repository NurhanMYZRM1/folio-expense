// One expense, as the desktop's expense table shows it, for native lists.
import { router } from "expo-router";
import { SymbolView } from "expo-symbols";
import { memo, useEffect, useState } from "react";
import { ActivityIndicator, Image, Pressable, StyleSheet, Text, useColorScheme, View } from "react-native";
import type { Expense } from "@folio/bindings/generated";
import { dateLabel, STATUS_LABELS } from "@folio/lib/constants";
import { formatMoney } from "@folio/lib/money";
import { call } from "~/core/folio";
import { palette, type Palette } from "./theme";

function statusColors(status: string, p: Palette) {
  if (status === "ready") return { fg: p.accent, bg: p.accentBg };
  if (status === "needs_review") return { fg: p.warning, bg: p.warningBg };
  return { fg: p.muted, bg: p.border };
}

export function StatusPill({ status }: { status: string }) {
  const p = palette[useColorScheme() === "dark" ? "dark" : "light"];
  const c = statusColors(status, p);
  return (
    <View style={[s.pill, { backgroundColor: c.bg }]}>
      {status === "extracting" ? <ActivityIndicator size={8} color={c.fg} /> : <View style={[s.dot, { backgroundColor: c.fg }]} />}
      <Text style={[s.pillText, { color: c.fg }]}>{STATUS_LABELS[status] ?? status}</Text>
    </View>
  );
}

function Thumbnail({ expense, p }: { expense: Expense; p: Palette }) {
  const [uri, setUri] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    if (expense.receiptId) {
      void call<string | null>("read_thumbnail", { id: expense.receiptId })
        .then((dataUrl) => live && setUri(dataUrl))
        .catch(() => undefined);
    }
    return () => {
      live = false;
    };
  }, [expense.receiptId, expense.updatedAt]);
  return (
    <View style={[s.thumb, { backgroundColor: p.accentBg }]}>
      {uri ? <Image source={{ uri }} style={StyleSheet.absoluteFill} resizeMode="cover" /> : <SymbolView name={expense.receiptId ? "doc.text" : "square.and.pencil"} size={18} tintColor={p.accent} />}
    </View>
  );
}

export const ExpenseRow = memo(function ExpenseRow({ expense }: { expense: Expense }) {
  const p = palette[useColorScheme() === "dark" ? "dark" : "light"];
  const title = expense.merchantName || expense.receiptFilename || "Untitled expense";
  const amount = expense.totalAmountMinor != null ? formatMoney(expense.totalAmountMinor, expense.currency ?? "MYR") : "—";
  return (
    <Pressable
      onPress={() => router.push(`/expenses/${expense.id}`)}
      style={({ pressed }) => [s.row, { backgroundColor: pressed ? p.border : p.surface }]}
      accessibilityRole="button"
      accessibilityLabel={`${title}, ${amount}, ${STATUS_LABELS[expense.status] ?? expense.status}`}
    >
      <Thumbnail expense={expense} p={p} />
      <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
        <Text numberOfLines={1} style={[s.title, { color: p.text }]}>
          {title}
        </Text>
        <Text numberOfLines={1} style={[s.sub, { color: p.muted }]}>
          {dateLabel(expense.occurredAt)} · {expense.category}
        </Text>
        <StatusPill status={expense.status} />
      </View>
      <Text style={[s.amount, { color: p.text }]}>{amount}</Text>
    </Pressable>
  );
});

const s = StyleSheet.create({
  row: { flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 16, paddingVertical: 12 },
  thumb: { width: 48, height: 60, borderRadius: 8, borderCurve: "continuous", overflow: "hidden", alignItems: "center", justifyContent: "center" },
  title: { fontSize: 16, fontWeight: "600" },
  sub: { fontSize: 13 },
  amount: { fontSize: 15, fontWeight: "600", fontVariant: ["tabular-nums"] },
  pill: { flexDirection: "row", alignSelf: "flex-start", alignItems: "center", gap: 5, paddingHorizontal: 8, paddingVertical: 3, borderRadius: 999, marginTop: 2 },
  dot: { width: 6, height: 6, borderRadius: 3 },
  pillText: { fontSize: 12, fontWeight: "600" },
});
