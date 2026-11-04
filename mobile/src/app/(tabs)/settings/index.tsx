import { MenuView } from "@expo/ui/community/menu";
import Constants from "expo-constants";
import { Stack } from "expo-router";
import { SymbolView } from "expo-symbols";
import * as WebBrowser from "expo-web-browser";
import { useColorScheme, View } from "react-native";
import Screen from "~/dom/pages/settings";
import { useFolioDomProps } from "~/native/dom-props";
import { palette } from "~/native/theme";

// Set in app.json `extra` once the pages are published (App Review 5.1.1(i) and 1.5).
const links = [
  { id: "privacy", title: "Privacy Policy", image: "hand.raised" as const, url: Constants.expoConfig?.extra?.privacyPolicyUrl },
  { id: "support", title: "Support", image: "questionmark.circle" as const, url: Constants.expoConfig?.extra?.supportUrl },
].filter((l): l is typeof l & { url: string } => typeof l.url === "string" && l.url.startsWith("https://"));

export default function Route() {
  const scheme = useColorScheme() === "dark" ? "dark" : "light";
  return (
    <>
      {links.length > 0 && (
        <Stack.Screen
          options={{
            headerRight: () => (
              <MenuView
                colorScheme={scheme}
                actions={links.map(({ id, title, image }) => ({ id, title, image }))}
                onPressAction={(e) => {
                  const link = links.find((l) => l.id === e.nativeEvent.event);
                  if (link) void WebBrowser.openBrowserAsync(link.url);
                }}
              >
                <View style={{ paddingHorizontal: 6 }} accessibilityRole="button" accessibilityLabel="Privacy and support">
                  <SymbolView name="info.circle" size={20} tintColor={palette[scheme].accent} />
                </View>
              </MenuView>
            ),
          }}
        />
      )}
      <Screen {...useFolioDomProps("/settings")} />
    </>
  );
}
