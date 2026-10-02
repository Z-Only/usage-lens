import type { Page, TestInfo } from "@playwright/test";

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
