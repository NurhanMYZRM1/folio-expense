// Native counterpart of ../src/components/motion.tsx: the same spring presets
// and entrance/exit choreography, built on Reanimated so it runs on the UI
// thread. Only transform and opacity animate, and every animation follows the
// system "Reduce Motion" setting (Reanimated's default, ReduceMotion.System).
import { useEffect, type ReactNode } from "react";
import { StyleSheet, View, type ViewStyle } from "react-native";
import Animated, { FadeIn, FadeInDown, FadeOut, LinearTransition, useAnimatedStyle, useSharedValue, withSpring } from "react-native-reanimated";

/** Spring presets shared with the desktop app (stiffness / damping / mass). */
export const spring = {
  /** Page and panel entrances: quick, a hint of overshoot. */
  snappy: { stiffness: 420, damping: 34, mass: 0.9 },
  /** Lists, cards, toasts: soft settle. */
  smooth: { stiffness: 300, damping: 30, mass: 1 },
  /** Playful pops: success, sheets. */
  bouncy: { stiffness: 520, damping: 22, mass: 0.8 },
} as const;

type Spring = (typeof spring)[keyof typeof spring];

/** Fade in and lift by `lift` points, settling on a spring. */
function rise(config: Spring, lift: number) {
  return FadeInDown.springify()
    .stiffness(config.stiffness)
    .damping(config.damping)
    .mass(config.mass)
    .withInitialValues({ opacity: 0, transform: [{ translateY: lift }] });
}

/** Entrance for the `index`th item of a list: staggered 35 ms, capped like the desktop. */
export function listEntering(index = 0) {
  return rise(spring.smooth, 8).delay(Math.min(index, 8) * 35);
}

/** Entrance for panels, cards and notices. */
export const panelEntering = rise(spring.snappy, 10);

/** Spring in from above, for the status pill at the top of the screen. */
export const dropEntering = FadeIn.springify().stiffness(spring.bouncy.stiffness).damping(spring.bouncy.damping).mass(spring.bouncy.mass);

/** Leave quickly so the next thing is never kept waiting (desktop: 120 ms). */
export const quickExit = FadeOut.duration(120);

/** Neighbours glide to their new places when an item appears or leaves. */
export const settle = LinearTransition.springify().stiffness(spring.smooth.stiffness).damping(spring.smooth.damping);

/** A panel that fades and lifts in on mount, and fades out quickly on removal. */
export function Reveal({ children, index = 0, style }: { children: ReactNode; index?: number; style?: ViewStyle | ViewStyle[] }) {
  return (
    <Animated.View entering={listEntering(index)} exiting={quickExit} layout={settle} style={style}>
      {children}
    </Animated.View>
  );
}

/**
 * Upload progress as a bar that fills on a spring. It scales a full-width
 * fill from the left edge (a transform) instead of animating its width,
 * which would re-run layout on every frame.
 */
export function ProgressBar({ value, color, track }: { value: number; color: string; track: string }) {
  const progress = useSharedValue(0);
  const width = useSharedValue(0);
  useEffect(() => {
    progress.value = withSpring(Math.max(0, Math.min(1, value)), spring.smooth);
  }, [value, progress]);
  const fill = useAnimatedStyle(() => ({
    // Scaling happens around the centre, so shift left by the part not yet filled.
    transform: [{ translateX: (-width.value * (1 - progress.value)) / 2 }, { scaleX: progress.value }],
  }));
  return (
    <View style={[s.track, { backgroundColor: track }]} onLayout={(e) => (width.value = e.nativeEvent.layout.width)}>
      <Animated.View style={[StyleSheet.absoluteFill, { backgroundColor: color }, fill]} />
    </View>
  );
}

const s = StyleSheet.create({
  track: { height: 6, borderRadius: 3, overflow: "hidden" },
});
