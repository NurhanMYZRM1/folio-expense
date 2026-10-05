// expenseapp:// links are handled by Expo Router on mobile.
export async function getCurrent(): Promise<string[] | null> {
  return null;
}

export async function onOpenUrl(_handler: (urls: string[]) => void): Promise<() => void> {
  return () => undefined;
}
