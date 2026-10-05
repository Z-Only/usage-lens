import { readFileSync } from "node:fs";
import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { captureOverflowDiagnostics, prepareFullPageCapture } from "./layout-diagnostics";

type Cell = { state: string; value: string | null };
type Attempt = {
  sourceId: string; attemptId: string; threadId: string; turnId: string; inferenceId: string;
  responseId: string | null; upstreamRequestId: string | null; status: string;
  startedAt: string; completedAt: string | null; timestampAnomaly: boolean;
  contentRetained: boolean;
  request: { model: Cell; reasoningEffort: Cell; serviceTier: Cell };
  tokens: Record<string, Cell> | null;
};
const fixtures = JSON.parse(readFileSync("crates/usage-lens-ui/tests/trace-fixtures.json", "utf8")) as {
  attempt: Attempt;
  list: { coverage: object; warnings: string[] };
  summary: { coverage: object; warnings: string[] };
};
const keys = ["inputTokens", "cachedInputTokens", "cacheWriteInputTokens", "outputTokens", "reasoningOutputTokens", "totalTokens"];
const scopeKeys = ["fromDate", "toDate", "threadId", "status", "requestedModel", "requestedReasoningEffort", "requestedServiceTier"];
function scope(url: URL) { return Object.fromEntries(scopeKeys.map(key => [key, url.searchParams.get(key)])); }
function attempts(): Attempt[] {
  const first = structuredClone(fixtures.attempt);
  first.attemptId = "earliest"; first.threadId = "thread-A";
  first.inferenceId = "inference-earliest"; first.responseId = "response-earliest"; first.upstreamRequestId = null;
  first.startedAt = "2026-10-01T12:00:00.000Z";
  first.completedAt = "2026-10-01T11:59:59.000Z"; first.timestampAnomaly = true;
  first.contentRetained = false;
  const second = structuredClone(first);
  second.attemptId = "later"; second.inferenceId = "inference-later"; second.responseId = null; second.startedAt = "2026-10-03T12:00:00.000Z";
  second.completedAt = null; second.timestampAnomaly = false;
  second.status = "incomplete"; second.tokens = null;
  second.request.serviceTier = { state: "omitted", value: null };
  const third = structuredClone(first);
  third.attemptId = "other-thread"; third.threadId = "thread-B";
  third.inferenceId = "inference-other"; third.responseId = "response-other";
  third.startedAt = "2026-10-03T12:00:00.000Z";
  third.completedAt = "2026-10-03T12:00:01.000Z"; third.timestampAnomaly = false;
  return [first, second, third];
}
function totals(rows: Attempt[]) {
  const tokenCoverage: Record<string, Record<string, string>> = {};
  const sums: Record<string, string | null> = {};
  for (const key of keys) {
    const counts = { reportedCount: 0, omittedCount: 0, notReportedCount: 0, invalidCount: 0 };
    let sum = 0n;
    for (const row of rows) {
      const cell = row.tokens?.[key];
      if (cell?.state === "reported") { counts.reportedCount++; sum += BigInt(cell.value!); }
      else if (cell?.state === "omitted") counts.omittedCount++;
      else if (cell?.state === "invalid") counts.invalidCount++;
      else counts.notReportedCount++;
    }
    tokenCoverage[key] = Object.fromEntries(Object.entries(counts).map(([name, value]) => [name, String(value)]));
    sums[key] = counts.reportedCount === 0 ? null : String(sum);
  }
  return { count: String(rows.length), tokenAttemptCount: String(rows.filter(row => row.tokens !== null).length), totals: sums, tokenCoverage, timestampAnomalyCount: String(rows.filter(row => row.timestampAnomaly).length) };
}
function filtered(url: URL) {
  return attempts().filter(row => {
    if (url.searchParams.has("threadId") && row.threadId !== url.searchParams.get("threadId")) return false;
    if (url.searchParams.has("status") && row.status !== url.searchParams.get("status")) return false;
    if (url.searchParams.has("fromDate") && row.startedAt.slice(0, 10) < url.searchParams.get("fromDate")!) return false;
    if (url.searchParams.has("toDate") && row.startedAt.slice(0, 10) > url.searchParams.get("toDate")!) return false;
    return ([["requestedModel", "model"], ["requestedReasoningEffort", "reasoningEffort"], ["requestedServiceTier", "serviceTier"]] as const)
      .every(([filter, key]) => !url.searchParams.has(filter) || (row.request[key].state === "reported" && row.request[key].value === url.searchParams.get(filter)));
  });
}
function grouped(rows: Attempt[], key: (row: Attempt) => string) {
  return [...new Set(rows.map(key))].sort().map(value => ({ value, rows: rows.filter(row => key(row) === value) }));
}
function summary(url: URL, truncated = false) {
  const rows = filtered(url);
  return {
    ...totals(rows), ...scope(url), source: { id: url.searchParams.get("sourceId") ?? "demo" }, attemptCount: String(rows.length),
    coverage: fixtures.summary.coverage, warnings: fixtures.summary.warnings,
    byStatus: grouped(rows, row => row.status).map(group => ({ ...totals(group.rows), status: group.value })),
    byThread: grouped(rows, row => row.threadId).map(group => ({
      ...totals(group.rows), threadId: group.value,
      firstStartedAt: group.rows.map(row => row.startedAt).sort()[0],
      lastStartedAt: group.rows.map(row => row.startedAt).sort().at(-1),
      statusCounts: Object.fromEntries(["completed", "failed", "cancelled", "incomplete"].map(status => [status, String(group.rows.filter(row => row.status === status).length)])),
    })),
    byRequestedSettings: grouped(rows, row => JSON.stringify(row.request)).map(group => ({ ...totals(group.rows), request: group.rows[0].request })),
    byDay: grouped(rows, row => row.startedAt.slice(0, 10)).map(group => ({ ...totals(group.rows), date: group.value })),
    byRequestedModel: [], byRequestedReasoningEffort: [], byRequestedServiceTier: [], byObservedModel: [], byObservedServiceTier: [],
    groupsTruncated: false, threadsTruncated: truncated, requestedSettingsTruncated: truncated, daysTruncated: truncated,
  };
}
async function noOverflow(page: Page, testInfo: TestInfo) {
  try { await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true); }
  catch (error) { await captureOverflowDiagnostics(page, testInfo); throw error; }
}
async function capture(page: Page, testInfo: TestInfo, name: string) {
  await noOverflow(page, testInfo);
  await prepareFullPageCapture(page, testInfo, name);
  const path = testInfo.outputPath(`${name}.png`);
  await page.screenshot({ path, fullPage: true });
  await testInfo.attach(name, { path, contentType: "image/png" });
}

for (const locale of ["en", "zh"] as const) {
  test(`thread summaries, joint settings and UTC days preserve exact local evidence in ${locale}`, async ({ page }, testInfo) => {
    const text = (en: string, zh: string) => locale === "en" ? en : zh;
    const requests: URL[] = []; const errors: string[] = []; const external: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    page.on("request", request => { if (new URL(request.url()).origin !== "http://127.0.0.1:4319") external.push(request.url()); });
    await page.addInitScript(({ locale }) => {
      localStorage.setItem("usage-lens:language", locale);
      localStorage.setItem("usage-lens:theme", locale === "zh" ? "dark" : "light");
    }, { locale });
    let truncated = false;
    await page.route(/\/api\/traces\/summary\?/, async route => {
      const url = new URL(route.request().url()); requests.push(url);
      await route.fulfill({ json: summary(url, truncated) });
    });
    await page.route(/\/api\/traces\?/, async route => {
      const url = new URL(route.request().url()); requests.push(url);
      const order = url.searchParams.get("order") ?? "newest_first";
      const rows = filtered(url).sort((left, right) => left.startedAt.localeCompare(right.startedAt) || left.attemptId.localeCompare(right.attemptId));
      if (order === "newest_first") rows.reverse();
      const offset = url.searchParams.has("cursor") ? 1 : 0;
      await route.fulfill({ json: {
        source: { id: url.searchParams.get("sourceId") }, ...scope(url), order,
        coverage: fixtures.list.coverage, warnings: fixtures.list.warnings,
        attempts: rows.slice(offset, offset + 1).map(row => ({ ...row, sourceId: url.searchParams.get("sourceId") })),
        nextCursor: offset === 0 && rows.length > 1 ? "synthetic-order-bound-cursor" : null,
      } });
    });
    await page.goto("/");
    await page.getByRole("navigation").getByRole("button", { name: text("Traces", "追踪"), exact: true }).click();
    const threads = page.locator(".trace-thread-summary");
    const settings = page.locator(".trace-settings-comparison");
    const days = page.locator(".trace-daily");
    const order = page.getByLabel(text("Timeline order", "时间线顺序"), { exact: true });
    await expect(order).toHaveAccessibleName(text("Timeline order", "时间线顺序"));
    await expect(order).toHaveValue("newest_first");
    await expect(threads).toContainText("thread-A"); await expect(threads).toContainText("thread-B");
    await expect(settings).toContainText("requested-model"); await expect(settings).toContainText("priority");
    await expect(settings).toContainText(text("Omitted", "已省略"));
    await expect(days).toContainText("2026-10-01"); await expect(days).toContainText("2026-10-03");
    await expect(days).not.toContainText("2026-10-02");
    await expect(page.locator(".trace-summary")).toContainText("18,014,398,509,481,986,646");
    await capture(page, testInfo, `trace-insights-${locale}`);
    // Thread navigation uses submitted filters, discards drafts, and starts a fresh oldest-first timeline.
    const model = page.getByLabel(text("Requested model (exact)", "请求模型（精确匹配）"), { exact: true });
    await model.fill("draft-only");
    const threadButton = threads.getByRole("button", { name: `${text("View thread", "查看会话")} thread-A`, exact: true });
    await threadButton.focus(); await threadButton.press("Enter");
    await expect(order).toHaveValue("oldest_first"); await expect(model).toHaveValue("");
    await expect(page.locator("#main-content")).toBeFocused();
    await expect(page.locator(".trace-attempt-id")).toHaveText("earliest");
    await expect(page.locator(".trace-reader")).toContainText(text("Clock anomaly", "时钟异常"));
    await expect(threads).not.toContainText("thread-B");
    await expect(page.locator(".trace-summary")).toContainText("9,007,199,254,740,993,323");
    const listRequest = requests.findLast(url => url.pathname === "/api/traces")!;
    expect(listRequest.searchParams.get("order")).toBe("oldest_first");
    expect(listRequest.searchParams.get("threadId")).toBe("thread-A");
    expect(listRequest.searchParams.has("cursor")).toBe(false);
    expect(requests.filter(url => url.pathname.endsWith("summary")).every(url => !url.searchParams.has("order"))).toBe(true);
    await order.selectOption("newest_first");
    await expect(page.locator(".trace-draft-notice")).toContainText(text("Unsubmitted edits", "尚有未提交的编辑"));
    await page.getByRole("button", { name: text("Load more traces", "加载更多追踪"), exact: true }).click();
    await expect(page.locator(".trace-attempt-id")).toHaveText(["earliest", "later"]);
    expect(requests.findLast(url => url.pathname === "/api/traces")?.searchParams.get("order")).toBe("oldest_first");
    await page.getByRole("button", { name: text("Apply trace filters", "应用追踪筛选"), exact: true }).click();
    await expect(page.locator(".trace-attempt-id")).toHaveText("later");
    expect(requests.findLast(url => url.pathname === "/api/traces")?.searchParams.has("cursor")).toBe(false);
    await capture(page, testInfo, `trace-thread-timeline-${locale}`);
    truncated = true;
    await page.getByRole("button", { name: text("Apply trace filters", "应用追踪筛选"), exact: true }).click();
    await expect(page.locator(".trace-threads-truncated")).toBeVisible();
    await expect(page.locator(".trace-settings-truncated")).toBeVisible();
    await expect(page.locator(".trace-days-truncated")).toBeVisible();
    await capture(page, testInfo, `trace-insights-truncated-${locale}`);
    truncated = false;
    await model.fill("absent-model");
    await page.getByRole("button", { name: text("Apply trace filters", "应用追踪筛选"), exact: true }).click();
    await expect(threads).not.toContainText("thread-A");
    await expect(page.locator(".trace-attempt")).toHaveCount(0);
    await expect(days.locator("tbody tr")).toHaveCount(0);
    await page.getByRole("button", { name: text("Clear all trace filters", "清除全部追踪筛选"), exact: true }).click();
    await expect(order).toHaveValue("newest_first");
    await expect(threads).toContainText("thread-B");
    await noOverflow(page, testInfo);
    expect(errors).toEqual([]); expect(external).toEqual([]);
  });
}
