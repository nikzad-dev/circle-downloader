import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";
import prettier from "eslint-config-prettier/flat";

const tsFiles = ["**/*.{ts,tsx}"];

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "src-tauri/target/**",
      "src-tauri/gen/**",
      "src/components/ui/**",
      "src/routeTree.gen.ts",
      "components.json",
    ],
  },
  js.configs.recommended,
  // Type-aware rules only for TypeScript files, which tsconfig.eslint.json covers.
  ...tseslint.configs.recommendedTypeChecked.map((config) => ({
    ...config,
    files: tsFiles,
  })),
  {
    files: tsFiles,
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        project: "./tsconfig.eslint.json",
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
    },
  },
  {
    // TanStack Router exports route objects, not just components.
    files: ["src/routes/**/*.{ts,tsx}"],
    rules: {
      "react-refresh/only-export-components": "off",
    },
  },
  {
    files: ["scripts/**/*.{js,mjs}"],
    languageOptions: {
      globals: globals.node,
    },
  },
  {
    files: ["**/*.test.{ts,tsx}", "src/test/**/*.{ts,tsx}"],
    rules: {
      "@typescript-eslint/require-await": "off",
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
    },
  },
  // Must stay last: turns off rules that conflict with Prettier.
  prettier,
);
