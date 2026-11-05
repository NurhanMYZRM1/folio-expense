const { defineConfig } = require("eslint/config");
const expoConfig = require("eslint-config-expo/flat");

module.exports = defineConfig([
  expoConfig,
  { ignores: ["dist/**", "ios/**", "android/**", "modules/folio-core/ios/Frameworks/**"] },
]);
