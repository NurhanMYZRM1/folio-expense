import { DarkTheme, DefaultTheme, type Theme } from "expo-router";

// The desktop app's tokens (../src/styles.css, WCAG AA contrast). Shadows are
// soft layered ones in place of hard borders: `shadow` for cards and buttons,
// `shadowPrimary` for the main action.
export const palette = {
  light: {
    bg: "#f4f6f7", surface: "#ffffff", text: "#1f2a31", muted: "#5d6973", border: "#e3e8eb",
    accent: "#2b6855", accentBg: "#e8f3ed", warning: "#845a14", warningBg: "#fbf3e2", danger: "#a8473d",
    shadow: "0 1px 2px rgba(20, 33, 41, 0.04), 0 4px 12px rgba(20, 33, 41, 0.05)",
    shadowPrimary: "0 1px 2px rgba(23, 66, 51, 0.2), 0 6px 14px rgba(46, 106, 86, 0.24)",
  },
  dark: {
    bg: "#101519", surface: "#182026", text: "#e4eaed", muted: "#9eabb4", border: "#2a353d",
    accent: "#93c7b0", accentBg: "#1f3a30", warning: "#e3c084", warningBg: "#372e1f", danger: "#ef9a8f",
    shadow: "0 1px 2px rgba(0, 0, 0, 0.3), 0 4px 12px rgba(0, 0, 0, 0.22)",
    shadowPrimary: "0 1px 2px rgba(0, 0, 0, 0.35), 0 6px 14px rgba(0, 0, 0, 0.3)",
  },
} as const;

export type Palette = (typeof palette)["light"] | (typeof palette)["dark"];

export function navTheme(scheme: "light" | "dark"): Theme {
  const p = palette[scheme];
  const base = scheme === "dark" ? DarkTheme : DefaultTheme;
  return { ...base, colors: { ...base.colors, primary: p.accent, background: p.bg, card: p.bg, text: p.text, border: p.border } };
}
