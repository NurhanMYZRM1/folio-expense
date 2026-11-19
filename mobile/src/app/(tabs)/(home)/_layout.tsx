import { router, Stack } from "expo-router";
import { SymbolView } from "expo-symbols";
import { Pressable, useColorScheme } from "react-native";
import { palette } from "~/native/theme";

export default function Layout() {
  const p = palette[useColorScheme() === "dark" ? "dark" : "light"];
  return (
    <Stack screenOptions={{ headerLargeTitle: true }}>
      <Stack.Screen
        name="index"
        options={{
          title: "Overview",
          headerRight: () => (
            <Pressable onPress={() => router.push("/settings")} hitSlop={12} style={{ paddingHorizontal: 6 }} accessibilityRole="button" accessibilityLabel="Settings">
              <SymbolView name={{ ios: "gearshape", android: "settings" }} size={20} tintColor={p.accent} />
            </Pressable>
          ),
        }}
      />
    </Stack>
  );
}
