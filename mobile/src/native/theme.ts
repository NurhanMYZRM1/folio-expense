import { DarkTheme, DefaultTheme, type Theme } from "expo-router";

// The desktop app's tokens (../src/styles.css).
export const palette = {
  light: { bg: "#f5f7f8", surface: "#ffffff", text: "#24313a", muted: "#7b8792", border: "#e5e9ec", accent: "#2d6a58", accentBg: "#edf5f0", warning: "#9a6c22", warningBg: "#fbf5e8", danger: "#b3261e" },
  dark: { bg: "#12171b", surface: "#1a2127", text: "#e2e8eb", muted: "#96a4ae", border: "#2b353e", accent: "#91c4ae", accentBg: "#233b32", warning: "#deba7b", warningBg: "#3b3223", danger: "#f2a29b" },
} as const;

export type Palette = (typeof palette)["light"] | (typeof palette)["dark"];

export function navTheme(scheme: "light" | "dark"): Theme {
  const p = palette[scheme];
  const base = scheme === "dark" ? DarkTheme : DefaultTheme;
  return { ...base, colors: { ...base.colors, primary: p.accent, background: p.bg, card: p.bg, text: p.text, border: p.border } };
}
