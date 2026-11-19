// Google and Apple sign-in normally return straight to the browser session that
// started them. A callback that reaches the app's own link handler instead is a
// repeat or belongs to a sign-in from before the app was closed: its code cannot
// be redeemed, so show Settings, where signing in can simply be started again.
import { Redirect } from "expo-router";

export default function AuthCallback() {
  return <Redirect href="/settings" />;
}
