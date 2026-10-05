/// <reference types="node" />
declare module "*.css";
declare module "*.woff";

// Vite asset URL imports in shared files Metro replaces (src/dom/shims).
declare module "*?url" {
  const url: string;
  export default url;
}
