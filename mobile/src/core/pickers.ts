// Receipts from the camera, the photo library or Files, copied under their
// original names so the Rust core records a readable filename.
import * as DocumentPicker from "expo-document-picker";
import { Directory, File, Paths } from "expo-file-system";
import * as ImagePicker from "expo-image-picker";

export type ReceiptSource = "camera" | "library" | "files";

export const toPath = (uri: string) => decodeURI(uri.replace(/^file:\/\//, ""));

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
      type: ["image/jpeg", "image/png", "application/pdf"],
    });
    return r.canceled ? [] : r.assets.map((a) => copyNamed(a.uri, a.name));
  }
  if (source === "camera") {
    const perm = await ImagePicker.requestCameraPermissionsAsync();
    if (!perm.granted) return [];
  }
  const options: ImagePicker.ImagePickerOptions = {
    mediaTypes: ["images"],
    quality: 0.9,
    // Folio stores originals; ask the system for a JPEG copy of HEIC photos.
    preferredAssetRepresentationMode: ImagePicker.UIImagePickerPreferredAssetRepresentationMode.Compatible,
  };
  const r =
    source === "camera"
      ? await ImagePicker.launchCameraAsync(options)
      : await ImagePicker.launchImageLibraryAsync({ ...options, allowsMultipleSelection: true, selectionLimit: 30 });
  if (r.canceled) return [];
  return r.assets.map((a, i) => {
    const base = (a.fileName ?? `Receipt-${stamp()}${i ? `-${i + 1}` : ""}`).replace(/\.[^.]+$/, "");
    return copyNamed(a.uri, `${base}.jpg`);
  });
}
