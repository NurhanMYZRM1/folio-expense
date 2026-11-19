// Background job progress and notices (the desktop's top bar and toasts).
import { useEffect, useState, useSyncExternalStore } from "react";
import { ActivityIndicator, Keyboard, Platform, Pressable, ScrollView, StyleSheet, Text, useColorScheme, useWindowDimensions, View } from "react-native";
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

/** How much of the bottom of the screen the keyboard covers, including over a web screen. */
function useKeyboardHeight() {
  const [height, setHeight] = useState(0);
  useEffect(() => {
    const ios = Platform.OS === "ios";
    const shown = Keyboard.addListener(ios ? "keyboardWillShow" : "keyboardDidShow", (e) => setHeight(e.endCoordinates.height));
    const hidden = Keyboard.addListener(ios ? "keyboardWillHide" : "keyboardDidHide", () => setHeight(0));
    return () => {
      shown.remove();
      hidden.remove();
    };
  }, []);
  return height;
}

export function StatusOverlay() {
  const p = palette[useColorScheme() === "dark" ? "dark" : "light"];
  const insets = useSafeAreaInsets();
  const window = useWindowDimensions();
  const keyboard = useKeyboardHeight();
  const busy = useSyncExternalStore(subscribe, () => processing);
  const list = useSyncExternalStore(subscribe, () => notices);
  const [visible, setVisible] = useState(busy);
  useEffect(() => setVisible(busy), [busy]);
  const side = { left: insets.left + 16, right: insets.right + 16 };
  // Above the tab bar, or above the keyboard when it is open.
  const bottom = keyboard ? keyboard + 12 : insets.bottom + 96;
  return (
    <View pointerEvents="box-none" style={StyleSheet.absoluteFill}>
      {visible && (
        // The row is the usable width; the pill is centred in it and can never be wider.
        <View pointerEvents="none" style={[s.pillRow, side, { top: insets.top + 4 }]}>
          <Animated.View
            entering={dropEntering}
            exiting={quickExit}
            style={[s.pill, { backgroundColor: p.surface, boxShadow: p.shadow }]}
            accessible
            accessibilityLabel={visible}
            accessibilityLiveRegion="polite"
          >
            <ActivityIndicator size="small" color={p.accent} />
            <Text numberOfLines={3} style={[s.pillText, { color: p.text }]}>{visible}</Text>
          </Animated.View>
        </View>
      )}
      <View pointerEvents="box-none" style={[s.toasts, side, { top: insets.top + 56, bottom }]}>
        {/* Toasts spring in and leave quickly; the others slide to fill the gap. */}
        {list.map((n) => (
          <Animated.View key={n.id} entering={panelEntering} exiting={quickExit} layout={settle} style={[s.toast, { backgroundColor: n.error ? p.danger : p.text }]}>
            {/* A long message scrolls inside its notice instead of covering the screen. */}
            <ScrollView style={{ flex: 1, maxHeight: window.height * 0.3 }} contentContainerStyle={s.toastBody} accessibilityRole="alert">
              <Text selectable style={[s.toastText, { color: n.error ? "#fff" : p.bg }]}>{n.text}</Text>
            </ScrollView>
            <Pressable onPress={() => dismiss(n.id)} hitSlop={8} style={s.close} accessibilityRole="button" accessibilityLabel="Dismiss notification">
              <Text style={[s.closeText, { color: n.error ? "#fff" : p.bg }]}>✕</Text>
            </Pressable>
          </Animated.View>
        ))}
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  pillRow: { position: "absolute", alignItems: "center" },
  pill: { maxWidth: "100%", flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 14, paddingVertical: 8, borderRadius: 18, borderCurve: "continuous" },
  // The text gives way and wraps; the spinner keeps its size.
  pillText: { flexShrink: 1, fontSize: 13, fontWeight: "500" },
  toasts: { position: "absolute", justifyContent: "flex-end", gap: 8 },
  toast: { flexShrink: 1, flexDirection: "row", alignItems: "flex-start", borderRadius: 14, borderCurve: "continuous", overflow: "hidden" },
  toastBody: { paddingLeft: 16, paddingVertical: 12 },
  toastText: { fontSize: 14, fontWeight: "500" },
  close: { minWidth: 44, minHeight: 44, alignItems: "center", justifyContent: "center" },
  closeText: { fontSize: 15, fontWeight: "600" },
});
