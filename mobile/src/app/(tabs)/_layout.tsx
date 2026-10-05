import { NativeTabs } from "expo-router/unstable-native-tabs";
import { useColorScheme } from "react-native";
import { palette } from "~/native/theme";

export default function TabsLayout() {
  const p = palette[useColorScheme() === "dark" ? "dark" : "light"];
  return (
    <NativeTabs tintColor={p.accent} minimizeBehavior="onScrollDown">
      <NativeTabs.Trigger name="(home)">
        <NativeTabs.Trigger.Label>Overview</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: "square.grid.2x2", selected: "square.grid.2x2.fill" }} md="dashboard" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="expenses">
        <NativeTabs.Trigger.Label>Expenses</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: "list.bullet.rectangle", selected: "list.bullet.rectangle.fill" }} md="receipt_long" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="import">
        <NativeTabs.Trigger.Label>Scan</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: "doc.viewfinder", selected: "doc.viewfinder.fill" }} md="document_scanner" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="claims">
        <NativeTabs.Trigger.Label>Claims</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: "folder", selected: "folder.fill" }} md="folder" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="settings">
        <NativeTabs.Trigger.Label>Settings</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf={{ default: "gearshape", selected: "gearshape.fill" }} md="settings" />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
