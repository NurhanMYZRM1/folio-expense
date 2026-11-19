import { Stack, ThemeProvider } from "expo-router";
import * as SplashScreen from "expo-splash-screen";
import { useEffect, useState } from "react";
import { AppState, ScrollView, Text, useColorScheme } from "react-native";
import { dataRoot, errorText, openCore, startCloudSync, useDataVersion } from "~/core/folio";
import { syncReminders } from "~/core/reminders";
import { startJobRunner } from "~/core/jobs";
import { notify, setProcessing, StatusOverlay } from "~/native/status";
import { navTheme, palette } from "~/native/theme";

void SplashScreen.preventAutoHideAsync();

// Opening straight on a pushed screen (the sign-in return lands on Settings) still leaves Back to the tabs.
export const unstable_settings = { anchor: "(tabs)" };

export default function RootLayout() {
  const version = useDataVersion();
  useEffect(() => {
    void syncReminders().catch(() => undefined);
  }, [version]);
  useEffect(() => {
    const listener = AppState.addEventListener("change", state => { if (state === "active") void syncReminders().catch(() => undefined); });
    return () => listener.remove();
  }, []);
  const scheme = useColorScheme() === "dark" ? "dark" : "light";
  const p = palette[scheme];
  const [state, setState] = useState<"opening" | "ready" | string>("opening");

  useEffect(() => {
    let stop: (() => void) | undefined;
    let closed = false;
    openCore()
      .then(() => {
        if (closed) return;
        setState("ready");
        const stopJobs = startJobRunner(setProcessing, notify);
        const stopCloud = startCloudSync();
        stop = () => { stopJobs(); stopCloud(); };
      })
      .catch((e) => { if (!closed) setState(errorText(e)); })
      .finally(() => void SplashScreen.hideAsync());
    return () => { closed = true; stop?.(); };
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
        <Stack.Screen name="settings" options={{ title: "Settings" }} />
        <Stack.Screen name="claims/[id]" options={{ title: "Claim" }} />
      </Stack>
      <StatusOverlay />
    </ThemeProvider>
  );
}
