import type { Page } from "@playwright/test";

import { expect, launchApp, test as base } from "./fixtures";

base.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// Light / Dark / System (#4658): Settings → Theme switches the whole app at once, the choice
// survives a restart (dark from the first paint), and System follows the OS preference.

const html = (page: Page) => page.locator("html");

/** What is actually painted: the page background's brightness (0–255) and the native scheme. */
async function painted(page: Page) {
  return page.evaluate(() => {
    // Any CSS colour → [r, g, b] through a 1×1 canvas (computed values may be oklch()).
    const ctx = document.createElement("canvas").getContext("2d")!;
    ctx.fillStyle = getComputedStyle(document.body).backgroundColor;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return {
      background: (r + g + b) / 3,
      scheme: document.documentElement.style.colorScheme,
    };
  });
}

async function openSettings(page: Page) {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  const group = page.getByRole("group", { name: "Theme" });
  await expect(group).toBeVisible();
  return {
    light: group.getByRole("button", { name: "Light", exact: true }),
    dark: group.getByRole("button", { name: "Dark", exact: true }),
    system: group.getByRole("button", { name: "System", exact: true }),
  };
}

base("the theme setting switches the app, sticks across a restart and follows the system", async ({
  dataDir,
}, testInfo) => {
  const port = 9300 + testInfo.workerIndex;

  const one = await launchApp({ dataDir, port });
  try {
    const page = one.page;
    // A fixed starting point: the machine running this may be in light or dark mode.
    await page.emulateMedia({ colorScheme: "light" });
    const theme = await openSettings(page);
    await expect(theme.system).toHaveAttribute("aria-pressed", "true");
    await expect(html(page)).not.toHaveClass(/dark/);

    await theme.dark.click();
    await expect(theme.dark).toHaveAttribute("aria-pressed", "true");
    await expect(html(page)).toHaveClass(/dark/);
    const dark = await painted(page);
    expect(dark.background).toBeLessThan(60);
    expect(dark.scheme).toBe("dark");

    // Every screen keeps it.
    for (const name of ["Dashboard", "Jobs", "Trucks"]) {
      await page.getByRole("link", { name, exact: true }).click();
      await expect(page.getByTestId("skeleton")).toHaveCount(0);
      await expect(html(page)).toHaveClass(/dark/);
      expect((await painted(page)).background).toBeLessThan(60);
    }
    expect(one.errors, "console errors").toEqual([]);
  } finally {
    await one.close();
  }

  // Restart on the same profile: dark again, without touching Settings.
  const two = await launchApp({ dataDir, port });
  try {
    const page = two.page;
    await page.emulateMedia({ colorScheme: "light" });
    await expect(html(page)).toHaveClass(/dark/);
    expect((await painted(page)).background).toBeLessThan(60);

    const theme = await openSettings(page);
    await expect(theme.dark).toHaveAttribute("aria-pressed", "true");

    await theme.light.click();
    await expect(html(page)).not.toHaveClass(/dark/);
    const light = await painted(page);
    expect(light.background).toBeGreaterThan(200);
    expect(light.scheme).toBe("light");

    // System: follows the OS preference, also while the app is open.
    await theme.system.click();
    await expect(theme.system).toHaveAttribute("aria-pressed", "true");
    await expect(html(page)).not.toHaveClass(/dark/);
    await page.emulateMedia({ colorScheme: "dark" });
    await expect(html(page)).toHaveClass(/dark/);
    await page.emulateMedia({ colorScheme: "light" });
    await expect(html(page)).not.toHaveClass(/dark/);
    expect(two.errors, "console errors").toEqual([]);
  } finally {
    await two.close();
  }
});
