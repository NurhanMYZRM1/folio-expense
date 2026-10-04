// Native redesign of ../src/features/expenses/Expenses.tsx: the register table
// becomes a virtualized list with a status segment control and header search.
import { MenuView } from "@expo/ui/community/menu";
import SegmentedControl from "@expo/ui/community/segmented-control";
import * as Haptics from "expo-haptics";
import { router, Stack, useLocalSearchParams } from "expo-router";
import { SymbolView } from "expo-symbols";
import { useMemo, useState } from "react";
import { FlatList, RefreshControl, StyleSheet, Text, useColorScheme, View } from "react-native";
import type { Expense } from "@folio/bindings/generated";
import { formatMoney, sumMinor } from "@folio/lib/money";
import { call, errorText } from "~/core/folio";
import { ExpenseRow } from "../expense-row";
import { notify } from "../status";
import { palette } from "../theme";
import { useFolio } from "../use-folio";

const SEGMENTS = [
  { key: "", label: "All" },
  { key: "needs_review", label: "Review" },
  { key: "ready", label: "Ready" },
  { key: "submitted", label: "Submitted" },
] as const;

export default function ExpensesScreen() {
  const scheme = useColorScheme() === "dark" ? "dark" : "light";
  const p = palette[scheme];
  const params = useLocalSearchParams<{ status?: string }>();
  const expenses = useFolio<Expense[]>("list_expenses");
  const [status, setStatus] = useState<string>(SEGMENTS.some((x) => x.key === params.status) ? (params.status ?? "") : "");
  const [query, setQuery] = useState("");
  const [refreshing, setRefreshing] = useState(false);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (expenses.data ?? [])
      .filter((e) => !status || e.status === status)
      .filter((e) => !q || [e.merchantName, e.description, e.receiptFilename, e.category].some((v) => v?.toLowerCase().includes(q)))
      .sort((a, b) => ((a.occurredAt ?? a.createdAt) < (b.occurredAt ?? b.createdAt) ? 1 : -1));
  }, [expenses.data, status, query]);

  const totals = useMemo(() => {
    const byCurrency = new Map<string, (number | null)[]>();
    for (const e of visible) {
      const c = e.currency ?? "MYR";
      byCurrency.set(c, [...(byCurrency.get(c) ?? []), e.totalAmountMinor]);
    }
    return [...byCurrency].map(([c, v]) => formatMoney(sumMinor(v), c)).join(" · ");
  }, [visible]);

  const manual = async () => {
    try {
      const e = await call<Expense>("create_expense");
      router.push(`/expenses/${e.id}`);
    } catch (err) {
      notify(errorText(err), true);
    }
  };

  return (
    <>
      <Stack.Screen
        options={{
          headerSearchBarOptions: { placeholder: "Search merchant, notes or receipt", onChangeText: (e) => setQuery(e.nativeEvent.text), tintColor: p.accent },
          headerRight: () => (
            <MenuView
              colorScheme={scheme}
              actions={[
                { id: "scan", title: "Scan a receipt", image: "doc.viewfinder" },
                { id: "manual", title: "Manual expense", image: "square.and.pencil" },
              ]}
              onPressAction={(e) => {
                void Haptics.selectionAsync();
                if (e.nativeEvent.event === "manual") void manual();
                else router.navigate("/import");
              }}
            >
              <View style={{ paddingHorizontal: 6 }} accessibilityLabel="Add expense">
                <SymbolView name="plus" size={20} tintColor={p.accent} />
              </View>
            </MenuView>
          ),
        }}
      />
      <FlatList
        style={{ flex: 1, backgroundColor: p.bg }}
        contentInsetAdjustmentBehavior="automatic"
        data={visible}
        keyExtractor={(e) => e.id}
        renderItem={({ item }) => <ExpenseRow expense={item} />}
        ItemSeparatorComponent={() => <View style={[s.separator, { backgroundColor: p.border }]} />}
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            tintColor={p.accent}
            onRefresh={async () => {
              setRefreshing(true);
              await expenses.reload();
              setRefreshing(false);
            }}
          />
        }
        ListHeaderComponent={
          <View style={s.header}>
            <SegmentedControl
              values={SEGMENTS.map((x) => x.label)}
              selectedIndex={SEGMENTS.findIndex((x) => x.key === status)}
              onChange={(e) => {
                void Haptics.selectionAsync();
                setStatus(SEGMENTS[e.nativeEvent.selectedSegmentIndex].key);
              }}
              appearance={scheme}
            />
            {expenses.data && (
              <Text style={[s.summary, { color: p.muted }]}>
                {visible.length} {visible.length === 1 ? "expense" : "expenses"}
                {visible.length ? ` · ${totals}` : ""}
              </Text>
            )}
          </View>
        }
        ListEmptyComponent={
          expenses.data ? (
            <View style={s.empty}>
              <SymbolView name="doc.text.magnifyingglass" size={34} tintColor={p.muted} />
              <Text style={[s.emptyText, { color: p.muted }]}>{query || status ? "No expenses match." : "Scan a receipt to start your expense register."}</Text>
            </View>
          ) : null
        }
        contentContainerStyle={{ paddingBottom: 120 }}
      />
    </>
  );
}

const s = StyleSheet.create({
  header: { paddingHorizontal: 16, paddingTop: 6, paddingBottom: 10, gap: 10 },
  summary: { fontSize: 13 },
  separator: { height: StyleSheet.hairlineWidth, marginLeft: 76 },
  empty: { alignItems: "center", gap: 12, paddingTop: 80, paddingHorizontal: 32 },
  emptyText: { fontSize: 15, textAlign: "center" },
});
