import { expect, type Page, type TestInfo } from "@playwright/test";

/** Attach real browser geometry without changing or replacing the failed assertion. */
export async function captureOverflowDiagnostics(page: Page, testInfo: TestInfo) {
  try {
    const geometry = await page.evaluate(() => {
      const name = (element: Element | null) =>
        element
          ? `${element.tagName.toLowerCase()}${element.id ? `#${element.id}` : ""}${
              element.classList.length ? `.${[...element.classList].join(".")}` : ""
            }`
          : null;
      const elements = [...document.querySelectorAll<HTMLElement>("body *")]
        .map((element) => {
          const rect = element.getBoundingClientRect();
          const style = getComputedStyle(element);
          if (rect.width === 0 || (rect.left >= 0 && rect.right <= innerWidth)) return null;
          const containers = [];
          for (let parent = element.parentElement; parent; parent = parent.parentElement) {
            const parentStyle = getComputedStyle(parent);
            if (parentStyle.overflowX !== "visible") {
              containers.push({
                element: name(parent),
                position: parentStyle.position,
                overflowX: parentStyle.overflowX,
                clientWidth: parent.clientWidth,
                scrollWidth: parent.scrollWidth,
              });
            }
          }
          return {
            element: name(element),
            text: element.textContent?.trim().slice(0, 100),
            rect: { left: rect.left, right: rect.right, top: rect.top, width: rect.width },
            position: style.position,
            overflowX: style.overflowX,
            clip: style.clip,
            offsetParent: name(element.offsetParent),
            clientWidth: element.clientWidth,
            scrollWidth: element.scrollWidth,
            containers,
          };
        })
        .filter((element) => element !== null);
      return {
        viewport: { width: innerWidth, height: innerHeight },
        document: {
          clientWidth: document.documentElement.clientWidth,
          scrollWidth: document.documentElement.scrollWidth,
          bodyScrollWidth: document.body.scrollWidth,
        },
        elements,
      };
    });
    await testInfo.attach("overflow-geometry.json", {
      body: JSON.stringify(geometry, null, 2),
      contentType: "application/json",
    });
    await testInfo.attach("overflow-full-page.png", {
      body: await page.screenshot({ fullPage: true }),
      contentType: "image/png",
    });
  } catch {
    // Diagnostics are best-effort; the caller rethrows the original test failure.
  }
}

/** Measure the real unfocused skip-link position without hiding or restyling it. */
export async function recordHiddenSkipLink(page: Page, testInfo: TestInfo, name: string) {
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

/** Normalize full-page capture origin without moving keyboard focus or changing CSS. */
export async function prepareFullPageCapture(page: Page, testInfo: TestInfo, name: string) {
  // A full-page capture from a scrolled viewport can reposition fixed offscreen UI.
  await page.evaluate(() => window.scrollTo(0, 0));
  await expect.poll(() => page.evaluate(() => ({ x: scrollX, y: scrollY }))).toEqual({ x: 0, y: 0 });
  await recordHiddenSkipLink(page, testInfo, name);
}
