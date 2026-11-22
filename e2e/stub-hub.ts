import { createServer, type IncomingMessage, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import type { Page } from "@playwright/test";

import { expect } from "./fixtures";

// A stub ProjectTrucks Hub served from Node, following the Hub's docs/api.md. Used by
// hub.spec.ts and gameplay-events.spec.ts (#5227). The app only talks to a Hub in a test
// profile when PROJECTTRUCKS_HUB_URL points at one, and never opens a browser there.

export const TOKEN = "1|e2e-token";

export class StubHub {
  approved = false;
  /** Every call answers 401, as after "Unlink" on the website. */
  revoked = false;
  tokenRevokedByApp = false;
  jobs = new Map<string, Record<string, unknown>>();
  deletes: string[] = [];
  /** Jobs the driver hid on the Hub (#4654). */
  hidden = new Set<string>();
  company: { name: string; slug: string; tag: string } | null = null;
  statuses: (Record<string, unknown> | "deleted")[] = [];
  private server: Server | null = null;

  get url() {
    const { port } = this.server!.address() as AddressInfo;
    return `http://127.0.0.1:${port}`;
  }

  async start() {
    this.server = createServer(async (req, res) => {
      const body = await readJson(req);
      const send = (status: number, data?: unknown) => {
        res.writeHead(status, { "content-type": "application/json" });
        res.end(data === undefined ? "" : JSON.stringify(data));
      };
      const path = req.url ?? "";

      if (req.method === "POST" && path === "/api/v1/device/code") {
        if (!String(body.client_name ?? "").startsWith("ProjectTrucks")) return send(422);
        return send(200, {
          device_code: "dev-code",
          user_code: "BCDF-2345",
          verification_uri: `${this.url}/link`,
          verification_uri_complete: `${this.url}/link?code=BCDF-2345`,
          expires_in: 900,
          interval: 1,
        });
      }
      if (req.method === "POST" && path === "/api/v1/device/token") {
        if (!this.approved) return send(400, { error: "authorization_pending" });
        return send(200, {
          access_token: TOKEN,
          token_type: "Bearer",
          user: { name: "Erin Example", username: "erin", profile_url: `${this.url}/drivers/erin` },
        });
      }

      if (req.headers.authorization !== `Bearer ${TOKEN}` || this.revoked) {
        return send(401, { message: "Unauthenticated." });
      }
      if (req.method === "POST" && path === "/api/v1/jobs") {
        const jobs = body.jobs as Record<string, unknown>[];
        for (const job of jobs) this.jobs.set(String(job.id), job);
        return send(200, {
          synced: jobs.length,
          jobs: jobs.map((j) => ({
            id: j.id,
            url: `${this.url}/jobs/${j.id}`,
            hidden: this.hidden.has(String(j.id)),
          })),
        });
      }
      if (req.method === "PATCH" && path.startsWith("/api/v1/jobs/")) {
        const id = decodeURIComponent(path.slice("/api/v1/jobs/".length));
        if (!this.jobs.has(id)) return send(404, { message: "Not found" });
        if (typeof body.hidden !== "boolean") return send(422, { message: "hidden" });
        if (body.hidden) this.hidden.add(id);
        else this.hidden.delete(id);
        return send(200, { id, url: `${this.url}/jobs/${id}`, hidden: body.hidden });
      }
      if (req.method === "GET" && path === "/api/v1/me") {
        return send(200, {
          id: 1,
          name: "Erin Example",
          username: "erin",
          avatar: null,
          company: this.company,
          profile_url: `${this.url}/drivers/erin`,
          profile_private: false,
          jobs_synced: this.jobs.size,
        });
      }
      if (req.method === "DELETE" && path.startsWith("/api/v1/jobs/")) {
        this.deletes.push(decodeURIComponent(path.slice("/api/v1/jobs/".length)));
        return send(204);
      }
      if (req.method === "PUT" && path === "/api/v1/status") {
        this.statuses.push(body);
        return send(200, { status: {} });
      }
      if (req.method === "DELETE" && path === "/api/v1/status") {
        this.statuses.push("deleted");
        return send(204);
      }
      if (req.method === "DELETE" && path === "/api/v1/token") {
        this.tokenRevokedByApp = true;
        return send(204);
      }
      send(404, { message: "Not found" });
    });
    await new Promise<void>((resolve) => this.server!.listen(0, "127.0.0.1", resolve));
  }

  async stop() {
    await new Promise<void>((resolve) => this.server?.close(() => resolve()));
  }
}

async function readJson(req: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  const text = Buffer.concat(chunks).toString();
  return text ? (JSON.parse(text) as Record<string, unknown>) : {};
}

/** Settings → ProjectTrucks Hub → Link account, approved on the stub. */
export async function link(page: Page, hub: StubHub) {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  const section = page.getByTestId("hub-settings");
  await section.getByRole("button", { name: "Link account" }).click();
  await expect(section.getByTestId("hub-code")).toHaveText("BCDF-2345");
  hub.approved = true;
  await expect(section.getByTestId("hub-linked")).toContainText("Linked as Erin Example (@erin)", {
    timeout: 15_000,
  });
}
