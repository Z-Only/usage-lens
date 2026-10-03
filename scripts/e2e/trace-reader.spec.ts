import { readFileSync } from "node:fs";
import { expect, test, type Locator, type Page, type TestInfo } from "@playwright/test";
import { captureOverflowDiagnostics } from "./layout-diagnostics";

type Cell = { state: string; value: string | null };
type Attempt = {
  sourceId: string; attemptId: string; status: string; contentRetained: boolean;
  request: { model: Cell; reasoningEffort: Cell; serviceTier: Cell };
  tokens: Record<string, Cell> | null;
};
type TraceFixtures = {
  attempt: Attempt;
  list: { source: { id: string }; attempts: Attempt[]; nextCursor: string | null; fromDate: string | null; toDate: string | null; coverage: { capture: string } };
  summary: { fromDate: string | null; toDate: string | null; attemptCount: string };
  detail: { attempt: Attempt; contentRetained: boolean; content: object | null };
};
const fixtures = JSON.parse(readFileSync("crates/usage-lens-ui/tests/trace-fixtures.json", "utf8")) as TraceFixtures;
const shell = JSON.parse(readFileSync("crates/usage-lens-ui/tests/fixtures.json", "utf8")) as {
  status: { sources: { id: string; displayName: string }[] };
  overview: { source: { id: string } };
};
function listFor(url: URL, attempts: Attempt[] = [structuredClone(fixtures.attempt)]) {
  const result = structuredClone(fixtures.list);
  result.source.id = url.searchParams.get("sourceId") ?? "demo";
  result.attempts = attempts.map(attempt => ({ ...attempt, sourceId: result.source.id }));
  result.fromDate = url.searchParams.get("fromDate");
  result.toDate = url.searchParams.get("toDate");
  return result;
}
function summaryFor(url: URL) {
  return { ...structuredClone(fixtures.summary), fromDate: url.searchParams.get("fromDate"), toDate: url.searchParams.get("toDate") };
}
async function summaryRoute(page: Page) {
  await page.route(/\/api\/traces\/summary\?/, async route => {
    await route.fulfill({ json: summaryFor(new URL(route.request().url())) });
  });
}
async function noOverflow(page: Page, testInfo: TestInfo) {
  try { await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true); }
  catch (error) { await captureOverflowDiagnostics(page, testInfo); throw error; }
}
async function captureProjection(page: Page, dialog: Locator, heading: string, visibleText: string, filename: string, testInfo: TestInfo) {
  const section = dialog.locator(".trace-projections .reader-section").filter({
    has: page.getByRole("heading", { name: heading, exact: true }),
  });
  const body = section.locator(".content-body");
  await expect(body).toContainText('"projection": "visible_text_only"');
  await expect(body).toContainText(visibleText);
  await section.scrollIntoViewIfNeeded();
  await body.scrollIntoViewIfNeeded();
  try {
    await expect(body).toBeInViewport({ ratio: 1 });
    await expect.poll(() => body.evaluate((element, expectedText) => {
      const drawer = element.closest("dialog")!;
      const drawerRect = drawer.getBoundingClientRect();
      const bodyRect = element.getBoundingClientRect();
      // Check the drawer's scrollport as well as the viewport: visibility alone
      // does not prove that a body below the drawer's fold appears in a capture.
      const clip = {
        left: Math.max(0, drawerRect.left + drawer.clientLeft),
        right: Math.min(innerWidth, drawerRect.left + drawer.clientLeft + drawer.clientWidth),
        top: Math.max(0, drawerRect.top + drawer.clientTop),
        bottom: Math.min(innerHeight, drawerRect.top + drawer.clientTop + drawer.clientHeight),
      };
      const inside = (rect: DOMRect, bounds: typeof clip) => rect.width > 0 && rect.height > 0
        && rect.left >= bounds.left - 1 && rect.right <= bounds.right + 1
        && rect.top >= bounds.top - 1 && rect.bottom <= bounds.bottom + 1;
      const textClip = {
        left: Math.max(clip.left, bodyRect.left + element.clientLeft),
        right: Math.min(clip.right, bodyRect.left + element.clientLeft + element.clientWidth),
        top: Math.max(clip.top, bodyRect.top + element.clientTop),
        bottom: Math.min(clip.bottom, bodyRect.top + element.clientTop + element.clientHeight),
      };
      const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
      let readableText = false;
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const start = node.textContent?.indexOf(expectedText) ?? -1;
        if (start < 0) continue;
        const range = document.createRange();
        range.setStart(node, start);
        range.setEnd(node, start + expectedText.length);
        const rects = [...range.getClientRects()];
        readableText = rects.length > 0 && rects.every(rect => inside(rect, textClip));
        break;
      }
      return {
        bodyInsideDrawerAndViewport: inside(bodyRect, clip),
        readableText: readableText && parseFloat(getComputedStyle(element).fontSize) >= 11,
        // A pre may intentionally scroll internally; its overflow must never
        // widen the containing drawer or push content off the page.
        drawerHasNoHorizontalOverflow: drawer.scrollWidth <= drawer.clientWidth + 1,
      };
    }, visibleText)).toEqual({
      bodyInsideDrawerAndViewport: true,
      readableText: true,
      drawerHasNoHorizontalOverflow: true,
    });
  } catch (error) { await captureOverflowDiagnostics(page, testInfo); throw error; }
  await noOverflow(page, testInfo);
  await page.screenshot({ path: testInfo.outputPath(filename), fullPage: false });
}

// Same-origin synthetic trace payloads only. Projects cover desktop, 390px and 320px.
for (const locale of ["en", "zh"] as const) {
  test(`trace projections and metadata remain distinct, escaped and readable in ${locale}`, async ({ page }, testInfo) => {
    const text = (en: string, zh: string) => locale === "en" ? en : zh;
    const errors: string[] = [];
    const external: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    page.on("request", request => { if (new URL(request.url()).origin !== "http://127.0.0.1:4319") external.push(request.url()); });
    await page.addInitScript(({ locale }) => {
      localStorage.setItem("usage-lens:language", locale);
      localStorage.setItem("usage-lens:theme", locale === "zh" ? "dark" : "light");
    }, { locale });
    await summaryRoute(page);
    await page.route(/\/api\/traces\?/, async route => {
      const attempts = ["completed", "failed", "cancelled", "incomplete"].map(status => {
        const attempt = structuredClone(fixtures.attempt);
        attempt.attemptId = `trace-${status}`;
        attempt.status = status;
        if (status !== "completed") { attempt.tokens = null; attempt.contentRetained = false; }
        return attempt;
      });
      const result = listFor(new URL(route.request().url()), attempts);
      result.nextCursor = null;
      await route.fulfill({ json: result });
    });
    await page.route(/\/api\/traces\/detail\?/, async route => {
      const id = new URL(route.request().url()).searchParams.get("attemptId") ?? "";
      const result = structuredClone(fixtures.detail);
      result.attempt.attemptId = id;
      if (id !== "trace-completed") { result.content = null; result.contentRetained = false; result.attempt.tokens = null; }
      await route.fulfill({ json: result });
    });
    await page.goto("/");
    await page.getByRole("navigation").getByRole("button", { name: text("Traces", "追踪"), exact: true }).click();
    await expect(page.locator(".trace-attempt")).toHaveCount(4);
    await expect(page.locator(".trace-summary")).toContainText("9,007,199,254,740,993,323");
    await expect(page.locator(".trace-summary")).toContainText(text("Observed response model", "观测响应模型"));
    await expect(page.locator(".trace-reader")).toContainText(text("not a physical HTTP request count", "不代表物理 HTTP 请求数量"));
    await expect(page.locator(".trace-summary")).toContainText(text("exact quota charges", "精确额度扣减"));
    await noOverflow(page, testInfo);
    await page.screenshot({ path: testInfo.outputPath(`trace-summary-${locale}.png`), fullPage: true });
    const opener = page.getByRole("button", { name: `${text("View trace", "查看追踪")} trace-completed`, exact: true });
    await opener.click();
    const dialog = page.getByRole("dialog", { name: text("Trace detail", "追踪详情"), exact: true });
    await expect(dialog.getByRole("heading", { name: text("Redacted visible-text projections", "脱敏可见文本投影"), exact: true })).toBeVisible();
    await expect(dialog).toContainText("<img src=x onerror=alert(1)>");
    await expect(dialog.locator("img")).toHaveCount(0);
    await expect(dialog).toContainText(text("Prepared request evidence only", "仅有已准备请求证据"));
    await expect(dialog).toContainText(text("not full raw requests or wire payloads", "不是完整原始请求或网络载荷"));
    await expect(dialog).toContainText("requested-model");
    await expect(dialog).toContainText("observed-model");
    await expect(dialog.getByRole("button", { name: text("Close trace detail", "关闭追踪详情") })).toBeFocused();
    await noOverflow(page, testInfo);
    // Preserve the initial metadata view, then capture each projection body.
    await page.screenshot({ path: testInfo.outputPath(`trace-projection-${locale}.png`), fullPage: false });
    await captureProjection(page, dialog, text("Prepared request projection", "已准备请求投影"),
      "synthetic visible prompt [REDACTED]", `trace-request-projection-${locale}.png`, testInfo);
    await captureProjection(page, dialog, text("Visible response projection", "可见响应投影"),
      "Synthetic visible response", `trace-response-projection-${locale}.png`, testInfo);
    await dialog.getByRole("button", { name: text("Back to traces", "返回追踪"), exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await expect(opener).toBeFocused();
    await opener.click();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(opener).toBeFocused();
    await page.getByRole("button", { name: `${text("View trace", "查看追踪")} trace-failed`, exact: true }).click();
    await expect(dialog).toContainText(text("No projection retained or available", "投影未保留或不可用"));
    await expect(dialog.locator(".trace-projections .content-body")).toHaveCount(0);
    await dialog.getByRole("button", { name: text("Close trace detail", "关闭追踪详情") }).click();
    expect(errors).toEqual([]);
    expect(external).toEqual([]);
  });
}

test("trace dates, pagination, validation and failed reads preserve submitted scope", async ({ page }) => {
  const requested: URL[] = [];
  let fail = false;
  let empty = false;
  await summaryRoute(page);
  await page.route(/\/api\/traces\?/, async route => {
    const url = new URL(route.request().url()); requested.push(url);
    if (fail) { await route.fulfill({ status: 503, json: { error: { code: "synthetic_trace_failure" } } }); return; }
    const result = listFor(url, empty ? [] : [{ ...structuredClone(fixtures.attempt), attemptId: url.searchParams.has("cursor") ? "trace-second" : "trace-first" }]);
    if (empty) result.coverage.capture = "not_captured";
    result.nextCursor = empty || url.searchParams.has("cursor") ? null : "trace-next";
    await route.fulfill({ json: result });
  });
  await page.clock.setFixedTime(new Date("2026-10-03T14:00:00Z"));
  await page.goto("/");
  await page.getByRole("navigation").getByRole("button", { name: "Traces", exact: true }).click();
  await expect(page.getByRole("button", { name: "View trace trace-first" })).toBeVisible();
  await page.getByRole("button", { name: "This UTC week", exact: true }).click();
  await expect(page.getByLabel("Trace from (UTC)", { exact: true })).toHaveValue("2026-09-28");
  await expect(page.getByLabel("Trace to (UTC)", { exact: true })).toHaveValue("2026-10-04");
  await page.getByLabel("Trace from (UTC)", { exact: true }).fill("2026-10-01");
  await page.getByLabel("Trace to (UTC)", { exact: true }).fill("2026-10-07");
  await page.getByRole("button", { name: "Apply trace dates", exact: true }).click();
  await expect(page.locator(".trace-reader .trace-scope")).toContainText("2026-10-01 – 2026-10-07");
  await page.getByLabel("Trace from (UTC)", { exact: true }).fill("2026-10-02");
  await page.getByRole("button", { name: "Load more traces", exact: true }).click();
  await expect(page.getByRole("button", { name: "View trace trace-second" })).toBeVisible();
  expect(requested.at(-1)?.searchParams.get("fromDate")).toBe("2026-10-01");
  expect(requested.at(-1)?.searchParams.get("cursor")).toBe("trace-next");
  await expect(page.getByLabel("Trace from (UTC)", { exact: true })).toHaveValue("2026-10-02");
  await page.getByLabel("Trace to (UTC)", { exact: true }).fill("2026-09-01");
  await page.getByRole("button", { name: "Apply trace dates", exact: true }).click();
  await expect(page.locator(".trace-reader")).toContainText("invalid_trace_date_range");
  await expect(page.locator(".trace-attempt")).toHaveCount(0);
  await page.locator(".trace-reader").getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator(".trace-reader")).toContainText("invalid_trace_date_range");
  fail = true;
  await page.getByLabel("Trace to (UTC)", { exact: true }).fill("2026-10-07");
  await page.locator(".trace-reader").getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator(".trace-reader")).toContainText("synthetic_trace_failure");
  fail = false; empty = true;
  await page.locator(".trace-reader").getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator(".trace-reader")).toContainText("Trace evidence not captured");
  await expect(page.locator(".trace-reader")).toContainText("No retained inference records");
  await expect(page.getByRole("button", { name: "Load more traces", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "All trace dates", exact: true }).click();
  await expect(page.getByLabel("Trace from (UTC)", { exact: true })).toHaveValue("");
});

test("delayed trace and detail responses cannot undo Close, new dates, navigation or source switches", async ({ page }) => {
  let releaseOld = () => {};
  const old = new Promise<void>(resolve => { releaseOld = resolve; });
  let delayedList = false;
  let detailStarted = false;
  let releaseDetail = () => {};
  const detailBarrier = new Promise<void>(resolve => { releaseDetail = resolve; });
  await page.route(/\/api\/status$/, async route => {
    const result = structuredClone(shell.status);
    result.sources.push({ ...result.sources[0], id: "other", displayName: "Other synthetic source" });
    await route.fulfill({ json: result });
  });
  await page.route(/\/api\/overview\?/, async route => {
    const result = structuredClone(shell.overview);
    result.source.id = new URL(route.request().url()).searchParams.get("sourceId") ?? "";
    await route.fulfill({ json: result });
  });
  await page.route(/\/api\/events\?/, async route => {
    await route.fulfill({ json: { source: { id: new URL(route.request().url()).searchParams.get("sourceId") }, events: [], nextCursor: null } });
  });
  await summaryRoute(page);
  await page.route(/\/api\/traces\?/, async route => {
    const url = new URL(route.request().url());
    const stale = url.searchParams.get("fromDate") === "2026-10-01";
    if (stale) { delayedList = true; await old; }
    const result = listFor(url, [{ ...structuredClone(fixtures.attempt), attemptId: stale ? "stale-attempt" : `trace-${url.searchParams.get("sourceId")}` }]);
    result.nextCursor = null;
    await route.fulfill({ json: result }).catch(() => {});
  });
  await page.route(/\/api\/traces\/detail\?/, async route => {
    detailStarted = true;
    await detailBarrier;
    const result = structuredClone(fixtures.detail); result.attempt.attemptId = "trace-demo";
    await route.fulfill({ json: result }).catch(() => {});
  });
  await page.goto("/");
  const nav = page.getByRole("navigation");
  await nav.getByRole("button", { name: "Traces", exact: true }).click();
  await page.getByRole("button", { name: "View trace trace-demo" }).click();
  await expect.poll(() => detailStarted).toBe(true);
  await page.getByRole("button", { name: "Close trace detail", exact: true }).click();
  releaseDetail();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.getByLabel("Trace from (UTC)", { exact: true }).fill("2026-10-01");
  await page.getByLabel("Trace to (UTC)", { exact: true }).fill("2026-10-07");
  await page.getByRole("button", { name: "Apply trace dates", exact: true }).click();
  await expect.poll(() => delayedList).toBe(true);
  await page.getByRole("button", { name: "All trace dates", exact: true }).click();
  await expect(page.getByRole("button", { name: "View trace trace-demo" })).toBeVisible();
  await nav.getByRole("button", { name: "Overview", exact: true }).click();
  releaseOld();
  await expect(page.getByRole("heading", { level: 1, name: "Your usage, in focus" })).toBeVisible();
  await nav.getByRole("button", { name: "Traces", exact: true }).click();
  await expect(page.getByRole("button", { name: "View trace stale-attempt" })).toHaveCount(0);
  await page.getByLabel("Source", { exact: true }).selectOption("other");
  await expect(page.getByRole("button", { name: "View trace trace-other" })).toBeVisible();
  await expect(page.getByRole("button", { name: "View trace trace-demo" })).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(0);
});
