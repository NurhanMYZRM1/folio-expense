import { Stack, ThemeProvider } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { useEffect, useState } from "react";
import { ScrollView, Text, useColorScheme } from "react-native";
import { dataRoot, errorText, openCore } from "~/core/folio";
import { startJobRunner } from "~/core/jobs";
import { notify, setProcessing, StatusOverlay } from "~/native/status";
import { navTheme, palette } from "~/native/theme";

void SplashScreen.preventAutoHideAsync();

export default function RootLayout() {
  const scheme = useColorScheme() === "dark" ? "dark" : "light";
  const p = palette[scheme];
  const [state, setState] = useState<"opening" | "ready" | string>("opening");

  useEffect(() => {
    let stop: (() => void) | undefined;
    openCore()
      .then(() => {
        setState("ready");
        stop = startJobRunner(setProcessing, notify);
      })
      .catch((e) => setState(errorText(e)))
      .finally(() => void SplashScreen.hideAsync());
    return () => stop?.();
  }, []);

  if (state === "opening") return null;
  if (state !== "ready") {
    return (
      <ScrollView style={{ flex: 1, backgroundColor: p.bg }} contentContainerStyle={{ padding: 24, paddingTop: 96, gap: 12 }}>
        <Text style={{ color: p.text, fontSize: 24, fontWeight: "600" }}>Folio could not open your workspace</Text>
        <Text selectable style={{ color: p.muted, fontSize: 15 }}>{state}</Text>
        <Text selectable style={{ color: p.muted, fontSize: 13 }}>Data folder: {dataRoot()}</Text>
      </ScrollView>
    );
  }

  return (
    <ThemeProvider value={navTheme(scheme)}>
      <Stack screenOptions={{ headerBackButtonDisplayMode: "minimal", contentStyle: { backgroundColor: p.bg } }}>
        <Stack.Screen name="(tabs)" options={{ headerShown: false, title: "Folio" }} />
        <Stack.Screen name="expenses/[id]" options={{ title: "Expense" }} />
        <Stack.Screen name="claims/[id]" options={{ title: "Claim" }} />
      </Stack>
      <StatusOverlay />
    </ThemeProvider>
  );
}
