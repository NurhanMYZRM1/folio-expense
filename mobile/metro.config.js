// The mobile app reuses the desktop app's React code from the parent folder
// (../src). Tauri APIs resolve to native shims; the data layer is the Rust
// core in modules/folio-core.
const { getDefaultConfig } = require("expo/metro-config");
const path = require("path");

const projectRoot = __dirname;
const repoRoot = path.resolve(projectRoot, "..");
const src = (p) => path.join(projectRoot, "src", p);
const shared = (p) => path.join(repoRoot, "src", p);

const config = getDefaultConfig(projectRoot);

config.watchFolders = [repoRoot];

const escape = (p) => p.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
config.resolver.blockList = [
  new RegExp(`^${escape(repoRoot)}/(node_modules|dist|src-tauri|target|public|tests|docs|\\.probe)/.*`),
  new RegExp(`^${escape(projectRoot)}/(ios|android)/.*`),
];
config.resolver.assetExts = [...config.resolver.assetExts, "woff", "woff2"];

// Desktop APIs used by the shared screens, backed by the native app.
const shims = {
  "@tauri-apps/api/core": src("dom/shims/tauri-core.ts"),
  "@tauri-apps/api/webviewWindow": src("dom/shims/tauri-webview-window.ts"),
  "@tauri-apps/plugin-dialog": src("dom/shims/tauri-dialog.ts"),
  "@tauri-apps/plugin-deep-link": src("dom/shims/tauri-deep-link.ts"),
};

// Shared modules replaced inside DOM components: background jobs run once,
// natively (src/core/jobs.ts); PDF pages are rendered by PDFKit.
const sharedReplacements = {
  [shared("lib/jobs.ts")]: src("dom/shims/jobs.ts"),
  [shared("lib/receiptRendering.ts")]: src("dom/shims/receipt-rendering.ts"),
};

const isBare = (name) => !name.startsWith(".") && !name.startsWith("/");
const insideMobile = (file) => file.startsWith(projectRoot + path.sep);

config.resolver.resolveRequest = (context, moduleName, platform) => {
  const shim = shims[moduleName];
  if (shim) return { type: "sourceFile", filePath: shim };

  if (moduleName.startsWith("@folio/")) {
    return context.resolveRequest(context, shared(moduleName.slice("@folio/".length)), platform);
  }
  if (moduleName.startsWith("~/")) {
    return context.resolveRequest(context, src(moduleName.slice(2)), platform);
  }

  // Packages imported by shared files resolve from mobile/node_modules.
  const resolved =
    isBare(moduleName) && !insideMobile(context.originModulePath)
      ? context.resolveRequest({ ...context, originModulePath: path.join(projectRoot, "package.json") }, moduleName, platform)
      : context.resolveRequest(context, moduleName, platform);

  const replacement = resolved.type === "sourceFile" && sharedReplacements[resolved.filePath];
  return replacement ? { type: "sourceFile", filePath: replacement } : resolved;
};

module.exports = config;
