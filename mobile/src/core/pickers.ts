// Receipts from the camera, the photo library or Files, copied under their
// original names so the Rust core records a readable filename.
import * as DocumentPicker from "expo-document-picker";
import { Directory, File, Paths } from "expo-file-system";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";
import * as ImagePicker from "expo-image-picker";

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

export async function pickReceipts(source: ReceiptSource): Promise<string[]> {
  if (source === "files") {
    const r = await DocumentPicker.getDocumentAsync({
      multiple: true,
      copyToCacheDirectory: true,
      type: ["image/jpeg", "image/png", "image/heic", "image/heif", "application/pdf"],
    });
    return r.canceled ? [] : r.assets.map((a) => copyNamed(a.uri, a.name));
  }
  if (source === "camera") {
    const perm = await ImagePicker.requestCameraPermissionsAsync();
    if (!perm.granted) return [];
    // The camera hands back a fresh JPEG; there is no original to keep.
    const r = await ImagePicker.launchCameraAsync({ mediaTypes: ["images"], quality: 0.9 });
    return r.canceled ? [] : [copyNamed(r.assets[0].uri, `Receipt-${stamp()}.jpg`)];
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
  const paths: string[] = [];
  for (const [i, a] of r.assets.entries()) {
    const base = (a.fileName ?? `Receipt-${stamp()}${i ? `-${i + 1}` : ""}`).replace(/\.[^.]+$/, "");
    const ext = extensionOf(a.uri);
    if (STORED_IMAGES.includes(ext)) {
      paths.push(copyNamed(a.uri, `${base}.${ext}`));
    } else {
      // Rarer formats (ProRAW, WebP, GIF, TIFF…) are converted to a JPEG Folio can store.
      const image = await ImageManipulator.manipulate(a.uri).renderAsync();
      const jpeg = await image.saveAsync({ format: SaveFormat.JPEG, compress: 0.9 });
      paths.push(copyNamed(jpeg.uri, `${base}.jpg`));
    }
  }
  return paths;
}
