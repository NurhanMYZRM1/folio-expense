import fs from "node:fs";
import path from "node:path";

// ../src/styles.css starts with `@import 'tailwindcss'` (unresolvable from the
// parent folder) and an @font-face served by Vite from /fonts. Inline it
// without those; src/dom/folio.css imports Tailwind itself.
const inlineSharedStyles = () => ({
  postcssPlugin: "folio-inline-shared-styles",
  Once(root, { parse }) {
    root.walkAtRules("import", (rule) => {
      const m = /^["'](.+\/src\/styles\.css)["']$/.exec(rule.params.trim());
      if (!m || !root.source?.input.file) return;
      const file = path.resolve(path.dirname(root.source.input.file), m[1]);
      const css = fs
        .readFileSync(file, "utf8")
        .replace(/@import\s+["']tailwindcss["'];?/, "")
        .replace(/@font-face\s*\{[^}]*\/fonts\/[^}]*\}/g, "");
      rule.replaceWith(parse(css, { from: file }).nodes);
    });
  },
});
inlineSharedStyles.postcss = true;

export default {
  plugins: [inlineSharedStyles, "@tailwindcss/postcss"],
};
