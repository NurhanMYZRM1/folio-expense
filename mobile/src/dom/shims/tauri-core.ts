// @tauri-apps/api/core for the shared screens: commands go to the Rust core
// through the native app (the same command names and arguments).
import { bridge } from "../bridge";

export const isTauri = () => true;

export async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const response = JSON.parse(await bridge().invoke(command, JSON.stringify(args))) as { result?: T; error?: unknown };
  if (response.error) throw response.error;
  return response.result as T;
}
