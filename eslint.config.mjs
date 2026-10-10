import antfu from '@antfu/eslint-config'

export default antfu(
  {
    react: true,

    typescript: {
      tsconfigPath: 'tsconfig.json',
    },

    formatters: {
      css: true,
      html: true,
      markdown: true,
    },

    ignores: [
      'src-tauri/target/**',
      'src-tauri/gen/**',
      'components.json',
      'src/components/ui/**',
      'src/routeTree.gen.ts',
    ],
  },

  // TanStack Router exports route objects, not just components.
  // Keep React Refresh checks enabled elsewhere.
  {
    files: ['src/routes/**/*.{ts,tsx}'],
    rules: {
      'react-refresh/only-export-components': 'off',
    },
  },
)
