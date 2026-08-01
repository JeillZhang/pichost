import { defineConfig, devices } from '@playwright/test'

const CI = !!process.env.CI

export default defineConfig({
  testDir: './e2e',
  /* Serial execution: tests share one backend instance, one database and one
   * Redis. Parallel workers would race on first-user-is-admin, invite-code
   * consumption and image counts. */
  fullyParallel: false,
  workers: 1,
  forbidOnly: CI,
  retries: CI ? 1 : 0,
  reporter: [
    ['list'],
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
  ],
  timeout: 60_000,
  expect: { timeout: 10_000 },
  outputDir: 'test-results',

  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL || 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  webServer: [
    // 1. Rust backend — reset the test DB/Redis first (Playwright starts
    //    webServer BEFORE globalSetup, so the reset must live in this command),
    //    then migrations auto-run at startup; env vars override config.toml.
    {
      command: 'node web-ui/e2e/reset-test-env.mjs && cargo run -p pichost-api',
      cwd: '..',
      port: 3000,
      reuseExistingServer: false,
      timeout: 180_000,
      env: {
        PICHOST_AUTH__JWT_SECRET:
          process.env.PICHOST_AUTH__JWT_SECRET || 'e2e-test-jwt-secret-at-least-32-bytes!!',
        PICHOST_SERVER__PUBLIC_URL:
          process.env.PICHOST_SERVER__PUBLIC_URL || 'http://localhost:3000',
        PICHOST_DATABASE_URL:
          process.env.PICHOST_DATABASE_URL ||
          'postgres://pichost:pichost@localhost:5432/pichost',
        PICHOST_REDIS_URL: process.env.PICHOST_REDIS_URL || 'redis://localhost:6379',
        PICHOST_STORAGE__LOCAL_BASE_PATH:
          process.env.PICHOST_STORAGE__LOCAL_BASE_PATH || '../storage-local-test',
        // E2E exercises many auth/upload requests per minute — raise the
        // per-window maxima so the suite is not throttled by default limits.
        // (double underscore = explicit nesting: RATE_LIMIT__AUTH_MAX → rate_limit.auth_max)
        PICHOST_RATE_LIMIT__AUTH_MAX: process.env.PICHOST_RATE_LIMIT__AUTH_MAX || '1000',
        PICHOST_RATE_LIMIT__UPLOAD_MAX: process.env.PICHOST_RATE_LIMIT__UPLOAD_MAX || '1000',
        PICHOST_RATE_LIMIT__GENERAL_MAX: process.env.PICHOST_RATE_LIMIT__GENERAL_MAX || '1000',
        PICHOST_RATE_LIMIT__PUBLIC_MAX: process.env.PICHOST_RATE_LIMIT__PUBLIC_MAX || '1000',
      },
    },
    // 2. Vite dev server (proxies /api and /u to the backend)
    {
      command: 'npm run dev -- --port 5173 --strictPort',
      port: 5173,
      reuseExistingServer: !CI,
      timeout: 60_000,
    },
  ],
})
