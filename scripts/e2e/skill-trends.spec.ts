import { readFileSync } from "node:fs";
import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { captureOverflowDiagnostics } from "./layout-diagnostics";

type SourceResponse = { source: { id: string } };
type Counts = {
  requested: string; loaded: string; invoked: string;
  loadedEvidence: { mainRead: string; instructionInjection: string; unknown: string };
};
type FixtureSet = {
  status: { sources: { id: string; displayName: string; mode: string }[] };
  overview: SourceResponse; events: SourceResponse; skills: SourceResponse; health: SourceResponse;
  skillSummary: SourceResponse & {
    fromDate: string | null; toDate: string | null; skillName: string | null;
    daily: unknown[]; totals: Counts;
  };
};
const fixtures = JSON.parse(readFileSync("crates/usage-lens-ui/tests/fixtures.json", "utf8")) as FixtureSet;

// Synthetic, same-origin responses only. No collection or settings mutation.
async function mockSources(page: Page) {
  await page.route(/\/api\/status$/, async route => {
    const response = structuredClone(fixtures.status);
    response.sources.push({ ...response.sources[0], id: "other", displayName: "Other synthetic source" });
    await route.fulfill({ json: response });
  });
  await page.route(/\/api\/(overview|events|skills|health)(?:\?|$)/, async route => {
    const url = new URL(route.request().url());
    const key = url.pathname.split("/").pop() as "overview" | "events" | "skills" | "health";
    const response = structuredClone(fixtures[key]);
    response.source.id = url.searchParams.get("sourceId") ?? "";
    await route.fulfill({ json: response });
  });
}
function summary(url: URL, count?: string) {
  const result = structuredClone(fixtures.skillSummary);
  result.source.id = url.searchParams.get("sourceId") ?? "";
  result.fromDate = url.searchParams.get("fromDate");
  result.toDate = url.searchParams.get("toDate");
  result.skillName = url.searchParams.get("skillName");
  if (count) result.totals.requested = count;
  return result;
}
async function openSkills(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Collection health & coverage" })).toHaveAttribute("aria-busy", "false");
  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("button", { name: "Skills", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Skill evidence", exact: true })).toBeVisible();
  // The legacy evidence response must settle before editing its sibling form.
  await expect(page.getByText("Loading local evidence…", { exact: true })).toHaveCount(0);
}
async function captureTrends(page: Page, testInfo: TestInfo, name: string) {
  await page.evaluate(() => window.scrollTo(0, 0));
  await expect.poll(() => page.evaluate(() => ({ x: scrollX, y: scrollY }))).toEqual({ x: 0, y: 0 });
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
  expect(geometry.scroll).toEqual({ x: 0, y: 0 });
  await testInfo.attach(`${name}-skip-link-geometry`, {
    body: JSON.stringify(geometry, null, 2), contentType: "application/json",
  });
  // Full-page capture at the document origin preserves the resting position of fixed controls.
  const path = testInfo.outputPath(`${name}.png`);
  await page.screenshot({ path, fullPage: true });
  await testInfo.attach(name, { path, contentType: "image/png" });
}
async function dates(page: Page, from = "2026-10-01", to = "2026-10-03") {
  await page.getByLabel("From (UTC)", { exact: true }).fill(from);
  await page.getByLabel("To (UTC)", { exact: true }).fill(to);
}

test("daily skill evidence keeps exact counts, unknown gaps, retry and bilingual mobile layout", async ({ page }, testInfo) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await mockSources(page);
  let fail = true;
  await page.route(/\/api\/skill-summary\?/, async route => {
    const url = new URL(route.request().url());
    expect(url.searchParams.get("fromDate")).toBe("2026-10-01");
    expect(url.searchParams.get("toDate")).toBe("2026-10-03");
    expect(url.searchParams.get("skillName")).toBe(" Exact &中文 ");
    expect(url.searchParams.has("model")).toBe(false);
    expect(url.searchParams.has("kind")).toBe(false);
    if (fail) await route.fulfill({ status: 503, json: { error: { code: "temporary_read_failure" } } });
    else await route.fulfill({ json: summary(url) });
  });
  await openSkills(page);
  let panel = page.getByRole("region", { name: "Daily skill evidence", exact: true });
  await expect(panel).toContainText("Apply a date range to read daily evidence");
  await dates(page);
  const exactName = panel.getByLabel("Exact skill name (optional)", { exact: true });
  await exactName.pressSequentially(" Exact &中文 ");
  await expect(exactName).toBeFocused();
  await expect(exactName).toHaveValue(" Exact &中文 ");
  await panel.getByRole("button", { name: "Apply filters", exact: true }).click();
  await expect(panel.getByRole("alert")).toContainText("Could not load daily evidence");
  fail = false;
  await panel.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(panel.locator(".skill-trend-totals dd").first()).toHaveText("9,007,199,254,740,993,123,456");
  await expect(panel.locator(".skill-trend-totals dd")).toHaveText(["9,007,199,254,740,993,123,456", "4", "1", "1", "2", "1"]);
  await expect(panel.locator("tbody tr")).toHaveCount(2);
  await expect(panel.locator("tbody th")).toHaveText(["2026-10-01", "2026-10-03"]);
  await expect(panel).toContainText("omitted days are unknown");
  await expect(panel).toContainText("Unknown occurrence time: 7");
  await expect(panel).toContainText("Across all retained records for this source and exact skill filter, outside the dated totals");
  const tableRegion = panel.getByRole("region", { name: "Daily skill evidence table", exact: true });
  await expect(tableRegion).toHaveCSS("overflow-x", "auto");
  await tableRegion.focus();
  await expect(tableRegion).toBeFocused();
  const noOverflow = async () => {
    try {
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    } catch (error) {
      await captureOverflowDiagnostics(page, testInfo);
      throw error;
    }
  };
  await noOverflow();
  await captureTrends(page, testInfo, "skills-en-light");
  await page.getByRole("button", { name: "Use dark theme", exact: true }).click();
  await page.getByLabel("Language", { exact: true }).selectOption("zh");
  panel = page.getByRole("region", { name: "每日技能证据", exact: true });
  await expect(panel).toContainText("加载 · 指令注入");
  await expect(panel).toContainText("未列出的日期为未知");
  await expect(panel).toContainText("不计入日期范围合计");
  await expect(panel.locator(".skill-trend-totals dd").first()).toHaveText("9,007,199,254,740,993,123,456");
  await noOverflow();
  await captureTrends(page, testInfo, "skills-zh-dark");
  expect(errors).toEqual([]);
});

test("daily skill filters reject invalid ranges, share Activity dates and reset honest empty results", async ({ page }) => {
  await mockSources(page);
  let requests = 0;
  await page.route(/\/api\/skill-summary\?/, async route => {
    requests++;
    const result = summary(new URL(route.request().url()));
    result.daily = [];
    result.totals = { requested: "0", loaded: "0", invoked: "0", loadedEvidence: { mainRead: "0", instructionInjection: "0", unknown: "0" } };
    await route.fulfill({ json: result });
  });
  await openSkills(page);
  const panel = page.getByRole("region", { name: "Daily skill evidence", exact: true });
  for (const [from, to] of [["2026-10-01", ""], ["2026-10-03", "2026-10-01"], ["2024-01-01", "2025-01-01"]]) {
    await dates(page, from, to);
    await panel.getByRole("button", { name: "Apply filters", exact: true }).click();
    await expect(panel.getByRole("alert")).toContainText("Choose a valid UTC date pair");
  }
  expect(requests).toBe(0);
  await dates(page);
  await panel.getByRole("button", { name: "Apply filters", exact: true }).click();
  await expect(panel).toContainText("No dated evidence in this range");
  await expect(panel).toContainText("does not prove no skill usage");
  await expect(panel.locator("table")).toHaveCount(0);
  const nav = page.getByRole("navigation", { name: "Main navigation" });
  await nav.getByRole("button", { name: "Activity", exact: true }).click();
  await expect(page.getByLabel("From", { exact: true })).toHaveValue("2026-10-01");
  await expect(page.getByLabel("To", { exact: true })).toHaveValue("2026-10-03");
  await nav.getByRole("button", { name: "Skills", exact: true }).click();
  await expect(panel).toContainText("No dated evidence in this range");
  await panel.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(panel.getByLabel("From (UTC)", { exact: true })).toHaveValue("");
  await expect(panel.getByLabel("To (UTC)", { exact: true })).toHaveValue("");
  await expect(panel).toContainText("Apply a date range to read daily evidence");
  await expect(panel.locator(".skill-trend-totals")).toHaveCount(0);
});

test("draft edits and source switches retire delayed daily evidence responses", async ({ page }) => {
  await mockSources(page);
  const releases: (() => void)[] = [];
  const completed: Promise<void>[] = [];
  await page.route(/\/api\/skill-summary\?/, async route => {
    const url = new URL(route.request().url());
    const old = url.searchParams.get("sourceId") === "demo" && url.searchParams.get("skillName") !== "new";
    let done = () => {};
    if (old) {
      completed.push(new Promise<void>(resolve => { done = resolve; }));
      await new Promise<void>(resolve => releases.push(resolve));
    }
    try {
      await route.fulfill({ json: summary(url, old ? "888888" : url.searchParams.get("sourceId") === "other" ? "42" : "9") });
    } finally { done(); }
  });
  await openSkills(page);
  const panel = page.getByRole("region", { name: "Daily skill evidence", exact: true });
  await dates(page);
  await panel.getByRole("button", { name: "Apply filters", exact: true }).click();
  await expect.poll(() => releases.length).toBe(1);
  await expect(panel.locator(".skill-trend-result")).toHaveAttribute("aria-busy", "true");
  await panel.getByLabel("Exact skill name (optional)", { exact: true }).fill("new");
  await expect(panel).toContainText("Apply a date range to read daily evidence");
  await panel.getByRole("button", { name: "Apply filters", exact: true }).click();
  await expect(panel.locator(".skill-trend-totals dd").first()).toHaveText("9");
  releases[0]();
  await completed[0];
  await expect(panel.locator(".skill-trend-totals dd").first()).toHaveText("9");
  await panel.getByLabel("Exact skill name (optional)", { exact: true }).fill("old");
  await panel.getByRole("button", { name: "Apply filters", exact: true }).click();
  await expect.poll(() => releases.length).toBe(2);
  await page.getByLabel("Source", { exact: true }).selectOption("other");
  await expect(panel.locator(".skill-trend-totals dd").first()).toHaveText("42");
  releases[1]();
  await completed[1];
  await expect(panel.locator(".skill-trend-totals dd").first()).toHaveText("42");
  await expect(panel).not.toContainText("888,888");
});
