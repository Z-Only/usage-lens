import { expect, test } from "@playwright/test";

// Read-only against the shared synthetic server. Every browser context has its own UI preferences.
test("local evidence flows, keyboard detail, locale and theme stay usable", async ({
  page,
}, testInfo) => {
  const pageErrors: string[] = [];
  const externalRequests: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.protocol !== "data:" && url.origin !== "http://127.0.0.1:4319")
      externalRequests.push(request.url());
  });
  const noOverflow = async () => {
    await expect
      .poll(() =>
        page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      )
      .toBe(true);
  };
  await page.goto("/");
  await expect(
    page.getByRole("heading", { level: 1, name: "Your usage, in focus" }),
  ).toBeVisible();
  await expect(
    page.getByText("演示数据 / Demo data", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: /^View event / }).first(),
  ).toBeVisible();
  await expect(page.locator("vite-error-overlay")).toHaveCount(0);
  await noOverflow();
  await page.screenshot({
    path: testInfo.outputPath("overview-en-light.png"),
    fullPage: true,
  });

  await page
    .getByRole("button", { name: /^View event / })
    .first()
    .click();
  await expect(
    page.getByRole("dialog", { name: "Event detail" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Close event detail" }),
  ).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);

  const nav = page.getByRole("navigation", { name: "Main navigation" });
  await nav.getByRole("button", { name: "Activity", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Recorded activity", exact: true }),
  ).toBeVisible();
  await page
    .getByLabel("Model", { exact: true })
    .fill("no-such-synthetic-model");
  await page
    .getByRole("button", { name: "Apply filters", exact: true })
    .click();
  await expect(page.getByRole("button", { name: /^View event / })).toHaveCount(
    0,
  );
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(
    page.getByRole("button", { name: /^View event / }).first(),
  ).toBeVisible();
  await noOverflow();

  await nav.getByRole("button", { name: "Skills", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Skill evidence", exact: true }),
  ).toBeVisible();
  await noOverflow();
  await nav.getByRole("button", { name: "Quotas", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Snapshot history", exact: true }),
  ).toBeVisible();
  await noOverflow();
  await nav.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Collection & privacy", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Delete local content", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await noOverflow();

  await nav.getByRole("button", { name: "Overview", exact: true }).click();
  await page
    .getByRole("button", { name: "Use dark theme", exact: true })
    .click();
  await page.getByLabel("Language", { exact: true }).selectOption("zh");
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(
    page.getByRole("heading", { level: 1, name: "用量，一目了然" }),
  ).toBeVisible();
  await noOverflow();
  await page.screenshot({
    path: testInfo.outputPath("overview-zh-dark.png"),
    fullPage: true,
  });
  await page.reload();
  await expect(
    page.getByRole("heading", { level: 1, name: "用量，一目了然" }),
  ).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await noOverflow();
  expect(pageErrors).toEqual([]);
  expect(externalRequests).toEqual([]);
});
