// What the native app gives the shared screens inside a DOM component.
export interface FolioBridge {
  invoke(command: string, args: string): Promise<string>;
  /** JSON options of @tauri-apps/plugin-dialog `open` → JSON array of paths, or null. */
  pick(options: string): Promise<string>;
  /** JSON array of JPEG data URLs for a stored PDF receipt. */
  renderPdf(relativePath: string): Promise<string>;
}

export function bridge(): FolioBridge {
  const b = (window as unknown as { __folio?: FolioBridge }).__folio;
  if (!b) throw new Error("Folio isn't connected to its data yet.");
  return b;
}
