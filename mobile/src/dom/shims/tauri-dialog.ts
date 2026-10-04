// @tauri-apps/plugin-dialog: receipts come from the camera, photo library or Files.
import { bridge } from "../bridge";

export async function open(options: Record<string, unknown> = {}): Promise<string[] | string | null> {
  return JSON.parse(await bridge().pick(JSON.stringify(options))) as string[] | null;
}
