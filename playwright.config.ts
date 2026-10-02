import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './scripts/e2e',
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  timeout: 30_000,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: 'http://127.0.0.1:4319',
    browserName: 'chromium',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'desktop', use: { viewport: { width: 1440, height: 1000 } } },
    { name: 'mobile-390', use: { viewport: { width: 390, height: 844 } } },
    { name: 'mobile-320', use: { viewport: { width: 320, height: 720 } } },
  ],
  webServer: {
    command: `${process.platform === 'win32' ? 'target/debug/usage-lens.exe' : 'target/debug/usage-lens'} serve --demo --port 4319`,
    url: 'http://127.0.0.1:4319',
    reuseExistingServer: false,
    timeout: 30_000,
    env: { USAGE_LENS_TEST_MODE: '1' },
  },
});
