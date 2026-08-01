import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    passWithNoTests: true,
    // E2E specs run under Playwright (npm run e2e), not vitest
    exclude: [...configDefaults.exclude, 'e2e/**', 'playwright-report/**', 'test-results/**'],
  },
})

import { configDefaults } from 'vitest/config'
