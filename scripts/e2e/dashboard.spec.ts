import { expect, test } from '@playwright/test';
import { captureOverflowDiagnostics } from './layout-diagnostics';

test('synthetic dashboard loads without overflow or runtime errors', async ({ page }, testInfo) => {
  const errors: string[] = [];
  const remoteRequests: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (!request.url().startsWith('http://127.0.0.1:4319/')) remoteRequests.push(request.url());
  });
  await page.goto('/');
  await expect(page).toHaveTitle(/Usage Lens/);
  await expect(page.getByText(/演示数据\s*\/\s*Demo data/).first()).toBeVisible();
  await expect(page.locator('main')).toBeVisible();
  try {
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  } catch (error) {
    await captureOverflowDiagnostics(page, testInfo);
    throw error;
  }
  expect(errors).toEqual([]);
  expect(remoteRequests).toEqual([]);
});
