import { requireNativeModule } from "expo";

interface FolioCoreNative {
  dataRoot(): string;
  open(root: string): Promise<string>;
  invoke(command: string, args: string): Promise<string>;
  recognizeText(path: string): Promise<{ text: string; confidence: number | null }>;
  renderPdfPages(path: string, maxPages: number, maxSide: number): Promise<string[]>;
}

export default requireNativeModule<FolioCoreNative>("FolioCore");
