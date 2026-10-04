// Receipts from the camera, the photo library or Files, copied under their
// original names so the Rust core records a readable filename.
import * as DocumentPicker from "expo-document-picker";
import { Directory, File, Paths } from "expo-file-system";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";
import * as ImagePicker from "expo-image-picker";
import type { ImportOutcome } from "@folio/bindings/generated";
import { errorText } from "./folio";

export type ReceiptSource = "camera" | "library" | "files";

export const toPath = (uri: string) => decodeURI(uri.replace(/^file:\/\//, ""));

/** Image formats the Rust core accepts as originals (receipt_storage::stage). */
const STORED_IMAGES = ["jpg", "jpeg", "png", "heic", "heif"];

const extensionOf = (name: string) => /\.([^./]+)$/.exec(name)?.[1]?.toLowerCase() ?? "";

function copyNamed(uri: string, name: string): string {
  const dir = new Directory(Paths.cache, "folio-import", `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
  dir.create({ intermediates: true, idempotent: true });
  const target = new File(dir, name.replace(/[/\\]/g, "_") || "receipt.jpg");
  new File(uri).copySync(target, { overwrite: true });
  return toPath(target.uri);
}

function stamp() {
  return new Date().toISOString().slice(0, 19).replace(/[-:T]/g, "");
}

/** Photos ready to import, plus any that could not be prepared (reported like import failures). */
export interface PickedReceipts {
  paths: string[];
  failed: ImportOutcome[];
}

const none: PickedReceipts = { paths: [], failed: [] };

/**
 * Lets the user pick receipts. `onPreparing` is called once the picker has
 * closed and the photos are being copied or converted, which can take a few
 * seconds for large or unusual formats.
 */
export async function pickReceipts(source: ReceiptSource, onPreparing?: () => void): Promise<PickedReceipts> {
  const picked = await pickFrom(source);
  if (!picked.length) return none;
  onPreparing?.();
  const result: PickedReceipts = { paths: [], failed: [] };
  // Each photo is prepared on its own: one that fails is reported and skipped,
  // and the rest of the selection still imports.
  for (const p of picked) {
    try {
      result.paths.push(await p.prepare());
    } catch (e) {
      result.failed.push({
        filename: p.name,
        expenseId: null,
        error: {
          code: "ImportFailed",
          message: `Folio couldn't read this photo (${errorText(e)}). Save it as JPEG or HEIC and try again.`,
          existingExpenseId: null,
        },
      });
    }
  }
  return result;
}

/** A chosen file and the step that copies (or converts) it for the Rust core. */
interface Picked {
  name: string;
  prepare: () => Promise<string>;
}

async function pickFrom(source: ReceiptSource): Promise<Picked[]> {
  if (source === "files") {
    const r = await DocumentPicker.getDocumentAsync({
      multiple: true,
      copyToCacheDirectory: true,
      type: ["image/jpeg", "image/png", "image/heic", "image/heif", "application/pdf"],
    });
    return r.canceled ? [] : r.assets.map((a) => ({ name: a.name, prepare: async () => copyNamed(a.uri, a.name) }));
  }
  if (source === "camera") {
    const perm = await ImagePicker.requestCameraPermissionsAsync();
    if (!perm.granted) return [];
    // The camera hands back a fresh JPEG; there is no original to keep.
    const r = await ImagePicker.launchCameraAsync({ mediaTypes: ["images"], quality: 0.9 });
    if (r.canceled) return [];
    const name = `Receipt-${stamp()}.jpg`;
    return [{ name, prepare: async () => copyNamed(r.assets[0].uri, name) }];
  }
  // Full quality + the current representation copies the photo untouched,
  // so iPhone photos arrive as HEIC and Folio stores the real original.
  const r = await ImagePicker.launchImageLibraryAsync({
    mediaTypes: ["images"],
    quality: 1,
    preferredAssetRepresentationMode: ImagePicker.UIImagePickerPreferredAssetRepresentationMode.Current,
    allowsMultipleSelection: true,
    selectionLimit: 30,
  });
  if (r.canceled) return [];
  return r.assets.map((a, i) => {
    const base = (a.fileName ?? `Receipt-${stamp()}${i ? `-${i + 1}` : ""}`).replace(/\.[^.]+$/, "");
    const ext = extensionOf(a.uri);
    if (STORED_IMAGES.includes(ext)) {
      const name = `${base}.${ext}`;
      return { name, prepare: async () => copyNamed(a.uri, name) };
    }
    // Rarer formats (ProRAW, WebP, GIF, TIFF…) are converted to a JPEG Folio can store.
    return {
      name: a.fileName ?? `${base}.${ext || "jpg"}`,
      prepare: async () => {
        const image = await ImageManipulator.manipulate(a.uri).renderAsync();
        const jpeg = await image.saveAsync({ format: SaveFormat.JPEG, compress: 0.9 });
        return copyNamed(jpeg.uri, `${base}.jpg`);
      },
    };
  });
}
