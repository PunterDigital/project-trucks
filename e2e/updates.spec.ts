import { createHash } from "node:crypto";
import { existsSync, readdirSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import path from "node:path";
import type { Page } from "@playwright/test";

import { expect, launchApp, test as base } from "./fixtures";

base.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// One-click updates (#3354, src-tauri/src/updater.rs). The app only checks a feed in a test
// profile when PROJECTTRUCKS_UPDATE_FEED points at one, so each test serves a fake GitHub
// "latest release" document from Node. Running a real installer is not covered here (it would
// close the app and replace the install); see the PR's manual check.

interface FakeRelease {
  tag: string;
  /** Bytes served as the `_x64-setup.exe`; `null` = the release has no installer asset. */
  installer: Buffer | null;
  /** Overrides the digest the feed publishes (to test a checksum mismatch). */
  digest?: string;
}

interface GhAsset {
  name: string;
  size: number;
  digest: string | null;
  browser_download_url: string;
}

class FakeFeed {
  release: FakeRelease = { tag: "v99.0.0", installer: Buffer.from("fake installer") };
  feedRequests = 0;
  downloads = 0;
  private server: Server | null = null;

  get url() {
    return `${this.base}/repos/PunterDigital/project-trucks/releases/latest`;
  }

  private get base() {
    const { port } = this.server!.address() as AddressInfo;
    return `http://127.0.0.1:${port}`;
  }

  get assetName() {
    return `ProjectTrucks_${this.release.tag.replace(/^v/, "")}_x64-setup.exe`;
  }

  async start() {
    this.server = createServer((req, res) => {
      const { release } = this;
      if (req.url?.endsWith("/releases/latest")) {
        this.feedRequests++;
        const assets: GhAsset[] = [
          {
            name: `ProjectTrucks_${release.tag.replace(/^v/, "")}_x64_en-US.msi`,
            size: 3,
            digest: null,
            browser_download_url: `${this.base}/download/app.msi`,
          },
        ];
        if (release.installer) {
          assets.push({
            name: this.assetName,
            size: release.installer.length,
            digest:
              release.digest ??
              `sha256:${createHash("sha256").update(release.installer).digest("hex")}`,
            browser_download_url: `${this.base}/download/${this.assetName}`,
          });
        }
        res.writeHead(200, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            tag_name: release.tag,
            html_url: `https://github.com/PunterDigital/project-trucks/releases/tag/${release.tag}`,
            draft: false,
            prerelease: false,
            assets,
          }),
        );
      } else if (req.url === `/download/${this.assetName}` && release.installer) {
        this.downloads++;
        res.writeHead(200, { "content-type": "application/octet-stream" });
        res.end(release.installer);
      } else {
        res.writeHead(404).end();
      }
    });
    await new Promise<void>((resolve) => this.server!.listen(0, "127.0.0.1", resolve));
  }

  async stop() {
    await new Promise<void>((resolve) => this.server?.close(() => resolve()));
  }
}

const test = base.extend<{ feed: FakeFeed; feedRelease: FakeRelease | null }>({
  feedRelease: [null, { option: true }],
  feed: async ({ feedRelease }, use) => {
    const feed = new FakeFeed();
    if (feedRelease) feed.release = feedRelease;
    await feed.start();
    await use(feed);
    await feed.stop();
  },
  appEnv: async ({ feed }, use) => {
    await use({ PROJECTTRUCKS_UPDATE_FEED: feed.url });
  },
});

async function openSettings(page: Page) {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Updates" })).toBeVisible();
}

test.describe("a newer release", () => {
  test("the launch check shows the banner; Later hides it", async ({ page, feed }) => {
    const banner = page.getByTestId("update-banner");
    await expect(banner).toContainText("ProjectTrucks v99.0.0 is available. You have v0.");
    await expect(banner.getByRole("button", { name: "Update to v99.0.0" })).toBeVisible();
    expect(feed.feedRequests).toBe(1);

    await openSettings(page);
    await expect(page.getByLabel("Check for updates")).toBeChecked();
    await expect(page.getByTestId("update-summary")).toHaveText("v99.0.0 is available.");

    await banner.getByRole("button", { name: "Later" }).click();
    await expect(banner).toHaveCount(0);
    // Nothing was downloaded without a click.
    expect(feed.downloads).toBe(0);
  });

  test.describe("with a bad checksum", () => {
    test.use({
      feedRelease: {
        tag: "v99.0.0",
        installer: Buffer.from("tampered installer"),
        digest: `sha256:${"0".repeat(64)}`,
      },
    });

    test("the download is refused and nothing is run", async ({ page, feed, dataDir }) => {
      const banner = page.getByTestId("update-banner");
      await banner.getByRole("button", { name: "Update to v99.0.0" }).click();
      await expect(banner.getByTestId("update-error")).toHaveText(
        "The download didn't match the release's checksum. Try again.",
      );
      await expect(banner.getByRole("button", { name: "Try again" })).toBeVisible();
      expect(feed.downloads).toBe(1);
      // Only the partial file is left in the test profile's update folder; the app is still up.
      const dir = path.join(dataDir, "data", "update");
      expect(readdirSync(dir)).toEqual([`${feed.assetName}.part`]);
      await expect(page.getByRole("heading", { name: "ProjectTrucks" })).toBeVisible();
    });
  });

  test.describe("without an installer asset", () => {
    test.use({ feedRelease: { tag: "v99.0.0", installer: null } });

    test("the banner falls back to the release page", async ({ page }) => {
      const banner = page.getByTestId("update-banner");
      await expect(banner).toContainText("ProjectTrucks v99.0.0 is available.");
      // Not clicked: it would open the real browser.
      await expect(banner.getByRole("button", { name: "Update available" })).toBeVisible();
      await expect(banner.getByRole("button", { name: /^Update to/ })).toHaveCount(0);
    });
  });
});

test.describe("up to date", () => {
  test.use({ feedRelease: { tag: "v0.0.1", installer: Buffer.from("old") } });

  test("no banner; Check now finds a release published later", async ({ page, feed }) => {
    await openSettings(page);
    const summary = page.getByTestId("update-summary");
    await expect(summary).toHaveText("You have the latest version.");
    await expect(page.getByTestId("update-banner")).toHaveCount(0);

    feed.release = { tag: "v99.1.0", installer: Buffer.from("new installer") };
    await page.getByRole("button", { name: "Check now" }).click();
    await expect(summary).toHaveText("v99.1.0 is available.");
    await expect(
      page.getByTestId("update-banner").getByRole("button", { name: "Update to v99.1.0" }),
    ).toBeVisible();
    expect(feed.feedRequests).toBe(2);
  });
});

test("turning checks off is saved, and the next launch doesn't check", async ({
  dataDir,
  feed,
}, testInfo) => {
  const port = 9300 + testInfo.workerIndex;
  const env = { PROJECTTRUCKS_UPDATE_FEED: feed.url };

  const first = await launchApp({ dataDir, port, env });
  try {
    await expect(first.page.getByTestId("update-banner")).toBeVisible();
    await openSettings(first.page);
    await first.page.getByLabel("Check for updates").click();
    await expect(first.page.getByLabel("Check for updates")).not.toBeChecked();
    expect(first.errors).toEqual([]);
  } finally {
    await first.close();
  }
  expect(feed.feedRequests).toBe(1);

  const second = await launchApp({ dataDir, port, env });
  try {
    await openSettings(second.page);
    await expect(second.page.getByLabel("Check for updates")).not.toBeChecked();
    await expect(second.page.getByTestId("update-summary")).toHaveText("Not checked yet.");
    await expect(second.page.getByTestId("update-banner")).toHaveCount(0);
    expect(feed.feedRequests).toBe(1);

    // Turning it back on checks straight away.
    await second.page.getByLabel("Check for updates").click();
    await expect(second.page.getByTestId("update-banner")).toBeVisible();
    expect(feed.feedRequests).toBe(2);
    expect(second.errors).toEqual([]);
  } finally {
    await second.close();
  }
});

base("a test profile without a feed never checks", async ({ page, dataDir }) => {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await expect(page.getByTestId("update-summary")).toHaveText(
    "Update checks are off in test profiles.",
  );
  await expect(page.getByRole("button", { name: "Check now" })).toBeDisabled();
  await expect(page.getByTestId("update-banner")).toHaveCount(0);
  expect(existsSync(path.join(dataDir, "data", "update"))).toBe(false);
});
