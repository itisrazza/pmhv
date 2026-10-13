import puppeteer, { type Browser, type Page } from "puppeteer-core";

const macChromium = "/Applications/Chromium.app/Contents/MacOS/Chromium";

function chromiumPath(): string {
  const configured = process.env.PMHV_CHROMIUM;
  if (configured) return configured;
  if (process.platform === "darwin") return macChromium;
  throw new Error(
    "set PMHV_CHROMIUM to the path of a Chromium or Chrome executable",
  );
}

export function launchBrowser(): Promise<Browser> {
  return puppeteer.launch({
    executablePath: chromiumPath(),
    headless: true,
    // CI runners on Ubuntu block the user namespaces Chrome's sandbox needs.
    args: process.env.CI ? ["--no-sandbox"] : [],
  });
}

/** Records anything that should fail a test: uncaught errors, console errors, HTTP errors. */
export function trackProblems(page: Page, problems: string[]): Page {
  page.on("pageerror", (error) =>
    problems.push(`page error on ${page.url()}: ${String(error)}`),
  );
  page.on("console", (message) => {
    if (message.type() === "error") {
      problems.push(`console error on ${page.url()}: ${message.text()}`);
    }
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      const request = response.request();
      problems.push(
        `HTTP ${response.status()} for ${request.method()} ${response.url()}`,
      );
    }
  });
  return page;
}

export async function bodyText(page: Page): Promise<string> {
  return page.evaluate(() => document.body.innerText);
}

export async function waitForText(
  page: Page,
  text: string,
  timeout = 30_000,
): Promise<void> {
  await page.waitForFunction(
    (wanted: string) => document.body.innerText.includes(wanted),
    { timeout, polling: 200 },
    text,
  );
}

/** Waits for the page title in the header bar, which pages portal in once their data loads. */
export async function waitForPageTitle(
  page: Page,
  title: string,
): Promise<void> {
  await page.waitForFunction(
    (wanted: string) =>
      [...document.querySelectorAll("span")].some(
        (span) =>
          span.textContent === wanted &&
          span.closest("[data-slot=sidebar]") === null,
      ),
    { polling: 200 },
    title,
  );
}

/** Replaces an input's value by typing, so React sees the change. */
export async function fill(
  page: Page,
  selector: string,
  value: string,
): Promise<void> {
  const input = await page.waitForSelector(selector);
  if (!input) throw new Error(`no element matches ${selector}`);
  await input.click({ count: 3 });
  await page.keyboard.press("Backspace");
  await input.type(value);
}
