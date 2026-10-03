import { readFileSync } from "node:fs";
import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { captureOverflowDiagnostics } from "./layout-diagnostics";

type FixtureSet = {
  status: { sources: { id: string; displayName: string; mode: string }[] };
  overview: { source: { id: string } };
};
const fixtures = JSON.parse(
  readFileSync("crates/usage-lens-ui/tests/fixtures.json", "utf8"),
) as FixtureSet;

async function recordHiddenSkipLink(page: Page, testInfo: TestInfo, name: string) {
  const link = page.locator("a.skip-link");
  await expect(link).not.toBeFocused();
  const geometry = await link.evaluate(element => {
    const rect = element.getBoundingClientRect();
    return {
      focused: element === document.activeElement,
      position: getComputedStyle(element).position,
      top: rect.top, bottom: rect.bottom, left: rect.left, right: rect.right,
      viewport: { width: innerWidth, height: innerHeight },
      scroll: { x: scrollX, y: scrollY },
    };
  });
  expect(geometry.focused).toBe(false);
  expect(geometry.bottom).toBeLessThanOrEqual(0);
  await testInfo.attach(`${name}-skip-link-geometry`, {
    body: JSON.stringify(geometry, null, 2), contentType: "application/json",
  });
}

// Every record is synthetic. Routes remain same-origin and do not touch real collectors.
function health(sourceId: string) {
  const captured = "2026-10-02T00:00:00Z";
  const stored = {
    count: "0",
    lastCapturedAt: captured,
    unknownOccurredAtCount: "0",
  };
  return {
    source: { id: sourceId, mode: "demo" },
    checkedAt: "2026-10-02T00:05:00Z",
    maxAgeMs: 900000,
    settings: { capturePaused: true, scope: "all_sources" },
    observations: [
      {
        method: "account/read",
        availability: "missing",
        capability: "unknown",
        freshness: { state: "unknown" },
        lastFailure: null,
      },
      {
        method: "account/usage/read",
        availability: "available",
        capability: "unsupported",
        freshness: { state: "stale", observedAt: captured },
        lastFailure: {
          errorCode: "method_not_found",
          attemptedAt: captured,
          state: "recent",
          atOrAfterLatestObservation: true,
        },
      },
      {
        method: "account/rateLimits/read",
        availability: "available",
        capability: "observed",
        freshness: { state: "future", observedAt: "2099-01-01T00:00:00Z" },
        lastFailure: null,
      },
    ],
    stored: {
      events: {
        ...stored,
        count: sourceId === "demo" ? "9007199254740993123456" : "987",
      },
      skills: stored,
      responseTokens: { count: null, lastCapturedAt: null },
      imports: { count: "1", lastImportedAt: captured },
    },
    coverage: { completeness: "partial", missingRecords: "unknown", preCollectionHistory: "unknown" },
    warnings: [],
  };
}

test("source health handles retry, interrupted source switching, and honest coverage", async ({ page }, testInfo) => {
  const pageErrors: string[] = [];
  page.on("pageerror", error => pageErrors.push(error.message));
  let mode: "fail" | "ready" | "delay" = "fail";
  let releaseOldResponse = () => {};
  const oldResponse = new Promise<void>(resolve => { releaseOldResponse = resolve; });
  await page.route(/\/api\/status$/, async route => {
    const status = structuredClone(fixtures.status);
    status.sources.push({ ...status.sources[0], id: "other", displayName: "Other synthetic source" });
    await route.fulfill({ json: status });
  });
  await page.route(/\/api\/overview\?/, async route => {
    const overview = structuredClone(fixtures.overview);
    overview.source.id = new URL(route.request().url()).searchParams.get("sourceId") ?? "";
    await route.fulfill({ json: overview });
  });
  await page.route(/\/api\/events\?/, async route => {
    const id = new URL(route.request().url()).searchParams.get("sourceId");
    await route.fulfill({ json: { source: { id }, events: [], nextCursor: null } });
  });
  await page.route(/\/api\/health\?/, async route => {
    const url = new URL(route.request().url());
    expect(url.searchParams.get("maxAgeMs")).toBe("900000");
    const sourceId = url.searchParams.get("sourceId") ?? "";
    if (mode === "fail") {
      await route.fulfill({ status: 503, json: { error: "read_failed" } });
      return;
    }
    if (mode === "delay" && sourceId === "demo") await oldResponse;
    await route.fulfill({ json: health(sourceId) });
  });
  await page.goto("/");
  const panel = page.getByRole("region", { name: "Collection health & coverage" });
  await expect(panel.getByText("Could not read collection health.", { exact: true })).toBeVisible();
  mode = "ready";
  await panel.getByRole("button", { name: "Retry health", exact: true }).click();
  await expect(panel.locator(".health-count > strong").first()).toHaveText("9,007,199,254,740,993,123,456");
  for (const text of ["Missing", "Unsupported", "Stale", "Future timestamp", "Last recorded error"]) {
    await expect(panel.getByText(text, { exact: true })).toBeVisible();
  }
  await expect(panel).toContainText("Zero records does not prove zero historical usage");
  await expect(panel).toContainText("No live account access check");
  await expect(panel.locator("progress")).toHaveCount(0);
  const noOverflow = async () => {
    try {
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    } catch (error) {
      await captureOverflowDiagnostics(page, testInfo);
      throw error;
    }
  };
  await noOverflow();
  const englishScreenshot = testInfo.outputPath("health-en-light.png");
  await recordHiddenSkipLink(page, testInfo, "health-en-light");
  // Oversized locator screenshots can recenter fixed offscreen controls into the crop.
  // Capture the document without changing application CSS or masking real elements.
  await page.screenshot({ path: englishScreenshot, fullPage: true });
  await testInfo.attach("health-en-light", { path: englishScreenshot, contentType: "image/png" });

  mode = "delay";
  const pending = page.waitForRequest(request => request.url().includes("/api/health?") && request.url().includes("sourceId=demo"));
  await panel.getByRole("button", { name: "Retry health", exact: true }).click();
  await pending;
  await expect(panel).toHaveAttribute("aria-busy", "true");
  await page.getByLabel("Source", { exact: true }).selectOption("other");
  await expect(panel.locator(".health-count > strong").first()).toHaveText("987");
  releaseOldResponse();
  await expect(panel).not.toContainText("9,007,199,254,740,993,123,456");
  await page.getByRole("button", { name: "Use dark theme", exact: true }).click();
  await page.getByLabel("Language", { exact: true }).selectOption("zh");
  const chinesePanel = page.getByRole("region", { name: "采集健康与覆盖" });
  await expect(chinesePanel).toContainText("未来时间戳");
  await expect(chinesePanel).toContainText("缺失记录：未知");
  await noOverflow();
  const chineseScreenshot = testInfo.outputPath("health-zh-dark.png");
  await recordHiddenSkipLink(page, testInfo, "health-zh-dark");
  await page.screenshot({ path: chineseScreenshot, fullPage: true });
  await testInfo.attach("health-zh-dark", { path: chineseScreenshot, contentType: "image/png" });
  expect(pageErrors).toEqual([]);
});

test("skip link stays offscreen until keyboard focus and reaches main content", async ({ page }, testInfo) => {
  await page.goto("/");
  await expect(page.getByText(/演示数据\s*\/\s*Demo data/).first()).toBeVisible();
  const link = page.getByRole("link", { name: "Skip to content", exact: true });
  await recordHiddenSkipLink(page, testInfo, "before-keyboard-focus");
  await page.keyboard.press("Tab");
  await expect(link).toBeFocused();
  await expect(link).toBeInViewport({ ratio: 1 });
  await testInfo.attach("skip-link-keyboard-focus", {
    body: await page.screenshot(), contentType: "image/png",
  });
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/#main-content$/);
  await expect(page.locator("main#main-content")).toBeFocused();
  await recordHiddenSkipLink(page, testInfo, "after-skip-navigation");
});
