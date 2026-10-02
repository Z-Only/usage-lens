import { expect, test } from '@playwright/test';

test('synthetic dashboard loads without overflow or runtime errors', async ({ page }) => {
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
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  expect(errors).toEqual([]);
  expect(remoteRequests).toEqual([]);
});
