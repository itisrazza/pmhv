// Headless-browser tests against a running pmhvd. Run through `just test-ui`, which starts an
// isolated pmhvd and passes its details in the environment. The tests run in order and share
// state: the first signs in, and later ones use that session.
//
// No screenshots: they trigger permission prompts on macOS. Everything is asserted through the DOM.

import { after, before, describe, it } from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import * as path from "node:path";
import { promisify } from "node:util";
import type { Browser, Page } from "puppeteer-core";
import {
  bodyText,
  fill,
  launchBrowser,
  trackProblems,
  waitForPageTitle,
  waitForText,
} from "./browser.ts";

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value)
    throw new Error(`${name} isn't set; run this through just test-ui`);
  return value;
}

const baseUrl = requiredEnv("PMHV_UI_URL");
const dataDir = requiredEnv("PMHV_DATA_DIR");
const binDir = requiredEnv("PMHV_BIN_DIR");

const desktop = { width: 1280, height: 800 };
const phone = { width: 500, height: 800 };
const bootTimeout = 60_000;
// Some firmware builds print only some of these, as in the boot integration test.
const firmwareOutput = ["Shell>", "BdsDxe", "UEFI", "Boot"];

const run = promisify(execFile);

async function pmhv(...args: string[]): Promise<string> {
  const { stdout } = await run(path.join(binDir, "pmhv"), args);
  return stdout;
}

describe("pmhv web UI", () => {
  let browser: Browser;
  let page: Page;
  const problems: string[] = [];

  const newPage = async () => {
    const created = trackProblems(await browser.newPage(), problems);
    await created.setViewport(desktop);
    return created;
  };
  const goto = (target: Page, route: string) =>
    target.goto(`${baseUrl}${route}`, { waitUntil: "load" });

  /** A test that also fails on any page error, console error or HTTP error along the way. */
  const uiTest = (name: string, body: () => Promise<void>) =>
    it(name, async () => {
      problems.length = 0;
      await body();
      assert.deepEqual(problems, [], "no errors in the browser");
    });

  before(async () => {
    browser = await launchBrowser();
    page = await newPage();
  });

  after(async () => {
    await browser?.close();
  });

  uiTest("signs in with the temporary password, then sets one", async () => {
    const temporary = (
      await readFile(path.join(dataDir, "initial-password.txt"), "utf8")
    ).trim();
    await goto(page, "/");
    await fill(page, "#password", temporary);
    await page.locator("button[type=submit]").click();

    await page.waitForSelector("#new-password");
    const password = "correct horse battery staple";
    await fill(page, "#new-password", password);
    await fill(page, "#confirm-password", password);
    await page.locator("button[type=submit]").click();

    await waitForPageTitle(page, "Machines");
  });

  uiTest("toggles the sidebar on desktop and phone widths", async () => {
    await page.setViewport(desktop);
    await goto(page, "/machines");
    const sidebar = "[data-slot=sidebar]:not([data-mobile])";
    await page.waitForSelector(`${sidebar}[data-state=expanded]`);
    // react-ui's CSS and the app's share a cascade layer; the app's `.hidden` once beat `md:block`.
    const layout = () =>
      page.$eval(sidebar, (element) => {
        const gap = element.querySelector("[data-slot=sidebar-gap]");
        return {
          display: getComputedStyle(element).display,
          gapWidth: gap?.getBoundingClientRect().width ?? -1,
        };
      });
    assert.deepEqual(await layout(), { display: "block", gapWidth: 256 });

    await page.locator("[data-sidebar=trigger]").click();
    await page.waitForSelector(`${sidebar}[data-state=collapsed]`);
    await page.waitForFunction(
      (selector: string) =>
        document
          .querySelector(`${selector} [data-slot=sidebar-gap]`)
          ?.getBoundingClientRect().width === 0,
      { polling: 100, timeout: 5_000 },
      sidebar,
    );

    await page.locator("[data-sidebar=trigger]").click();
    await page.waitForSelector(`${sidebar}[data-state=expanded]`);

    await page.setViewport(phone);
    await page.locator("[data-sidebar=trigger]").click();
    await page.waitForSelector("[data-slot=sidebar][data-mobile=true]", {
      visible: true,
    });
    await page.keyboard.press("Escape");
    await page.waitForSelector("[data-slot=sidebar][data-mobile=true]", {
      hidden: true,
    });
    await page.setViewport(desktop);
  });

  const pages = [
    { route: "/machines", title: "Machines" },
    { route: "/images", title: "Disk images" },
    { route: "/networks", title: "Networks" },
    { route: "/settings", title: "Settings" },
  ];
  for (const { route, title } of pages) {
    uiTest(`renders ${route}`, async () => {
      await goto(page, route);
      await waitForPageTitle(page, title);
    });
  }

  uiTest(
    "creates a text-console machine, boots it and force-stops it",
    async () => {
      const name = "UI text console";
      await goto(page, "/machines");
      await page.locator("button[title='New machine']").click();
      await fill(page, "#name", name);
      await page.locator("#console").click();
      await page.locator("[role=option]::-p-text('Text (serial)')").click();
      await fill(page, "#disk-size", "1");
      await page.locator("button[form=create-machine]").click();

      await page.waitForFunction(
        () => /^\/machines\/[^/]+$/.test(location.pathname),
        { polling: 100 },
      );
      const id = new URL(page.url()).pathname.split("/").at(-1) ?? "";
      await waitForPageTitle(page, name);

      await page.locator("button[title=Start]").click();
      await waitForText(page, "Running");

      const consolePage = await newPage();
      await goto(consolePage, `/machines/${id}/console`);
      await consolePage.waitForFunction(
        (needles: string[]) => {
          const rows = document.querySelector<HTMLElement>(".xterm-rows");
          return needles.some((needle) => rows?.innerText.includes(needle));
        },
        { timeout: bootTimeout, polling: 500 },
        firmwareOutput,
      );
      await consolePage.close();

      await page.locator("button[title='Add to favourites']").click();
      await page.waitForSelector(
        `[data-slot=sidebar] a[href="/machines/${id}"]`,
      );
      const favourites = await page.$eval(
        "[data-slot=sidebar]",
        (sidebar) => (sidebar as HTMLElement).innerText,
      );
      assert.match(favourites, new RegExp(`Favourites\\s+${name}`));

      await page.locator("button[title='More actions']").click();
      await page.locator("[role=menuitem]::-p-text('Force stop')").click();
      await waitForText(page, "Stopped");
    },
  );

  uiTest("shows a graphical console with the firmware on screen", async () => {
    const name = "UI graphical console";
    const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
    const spec = path.join(dataDir, "graphical.toml");
    await writeFile(
      spec,
      [
        `name = "${name}"`,
        `guest_os = "other"`,
        `backend = "qemu"`,
        `arch = "${arch}"`,
        `firmware = "uefi"`,
        `cpu_cores = 1`,
        `memory_mib = 512`,
        `network = "none"`,
        `console = "graphical"`,
        "",
      ].join("\n"),
    );
    await pmhv("create", spec);
    await pmhv("start", name);
    const machine = JSON.parse(await pmhv("--json", "show", name)) as {
      id: string;
    };

    try {
      const consolePage = await newPage();
      await goto(consolePage, `/machines/${machine.id}/console`);
      await consolePage.waitForFunction(
        () => {
          const canvas = document.querySelector("canvas");
          if (!canvas || canvas.width === 0 || canvas.height === 0) {
            return false;
          }
          const pixels = canvas
            .getContext("2d")
            ?.getImageData(0, 0, canvas.width, canvas.height).data;
          // Any colour channel lit, ignoring alpha.
          return pixels?.some((value, index) => index % 4 !== 3 && value > 0);
        },
        { timeout: bootTimeout, polling: 500 },
      );
      const text = await bodyText(consolePage);
      assert.doesNotMatch(text, /Disconnected|isn't running/);

      await consolePage.locator("button::-p-text(Keys)").click();
      await consolePage.waitForSelector(
        "[role=menuitem]::-p-text('Ctrl+Alt+Del')",
      );
      await consolePage.keyboard.press("Escape");
      await consolePage.close();
    } finally {
      await pmhv("force-stop", name);
    }
  });
});
