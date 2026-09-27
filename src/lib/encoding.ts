export function decodeBase64(value: string): Uint8Array {
  const binary = atob(value),
    bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
export function encodeBase64(value: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < value.length; i += 32768)
    binary += String.fromCharCode(...value.subarray(i, i + 32768));
  return btoa(binary);
}
