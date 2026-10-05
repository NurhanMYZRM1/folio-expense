// Background job progress and notices (the desktop's top bar and toasts).
import { useEffect, useState, useSyncExternalStore } from "react";
import { ActivityIndicator, Pressable, StyleSheet, Text, useColorScheme, View } from "react-native";
import Animated from "react-native-reanimated";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { dropEntering, panelEntering, quickExit, settle } from "./motion";
import { palette } from "./theme";

type Notice = { id: number; text: string; error: boolean };

let processing: string | null = null;
let notices: Notice[] = [];
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function setProcessing(text: string | null) {
  processing = text;
  emit();
}

export function notify(text: string, error = false) {
  const id = Date.now() + Math.random();
  notices = [...notices.slice(-2), { id, text, error }];
  emit();
  if (!error) setTimeout(() => dismiss(id), 6000);
}

function dismiss(id: number) {
  notices = notices.filter((n) => n.id !== id);
  emit();
}

export function StatusOverlay() {
  const p = palette[useColorScheme() === "dark" ? "dark" : "light"];
  const insets = useSafeAreaInsets();
  const busy = useSyncExternalStore(subscribe, () => processing);
  const list = useSyncExternalStore(subscribe, () => notices);
  const [visible, setVisible] = useState(busy);
  useEffect(() => setVisible(busy), [busy]);
  return (
    <View pointerEvents="box-none" style={StyleSheet.absoluteFill}>
      {visible && (
        <Animated.View
          entering={dropEntering}
          exiting={quickExit}
          style={[s.pill, { top: insets.top + 4, backgroundColor: p.surface, boxShadow: p.shadow }]}
          accessibilityLiveRegion="polite"
        >
          <ActivityIndicator size="small" color={p.accent} />
          <Text style={[s.pillText, { color: p.text }]}>{visible}</Text>
        </Animated.View>
      )}
      <View pointerEvents="box-none" style={[s.toasts, { bottom: insets.bottom + 96 }]}>
        {/* Toasts spring in and leave quickly; the others slide to fill the gap. */}
        {list.map((n) => (
          <Animated.View key={n.id} entering={panelEntering} exiting={quickExit} layout={settle}>
            <Pressable onPress={() => dismiss(n.id)} style={[s.toast, { backgroundColor: n.error ? p.danger : p.text }]} accessibilityRole="alert">
              <Text style={[s.toastText, { color: n.error ? "#fff" : p.bg }]}>{n.text}</Text>
            </Pressable>
          </Animated.View>
        ))}
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  pill: { position: "absolute", alignSelf: "center", flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 14, paddingVertical: 8, borderRadius: 999 },
  pillText: { fontSize: 13, fontWeight: "500" },
  toasts: { position: "absolute", left: 16, right: 16, gap: 8 },
  toast: { paddingHorizontal: 16, paddingVertical: 12, borderRadius: 14, borderCurve: "continuous" },
  toastText: { fontSize: 14, fontWeight: "500" },
});
