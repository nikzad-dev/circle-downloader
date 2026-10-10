// eslint.config.mjs
import antfu from '@antfu/eslint-config'

export default antfu({
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
    'src-tauri/target',
    'src-tauri/gen',
    'components.json',
    'src/components/ui/*.tsx',
  ],
})
