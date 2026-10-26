import { useState } from "react";

import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { downloadPercent, publishUpdateStatus, useUpdateStatus } from "@/lib/updates";

/**
 * "Update to vX.Y.Z" under the header when the updater found a newer release. On Windows one
 * click downloads the installer, runs it in passive mode and closes the app; elsewhere the
 * button opens the release page. "Later" hides it until the next launch.
 */
export function UpdateBanner() {
  const status = useUpdateStatus();
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const latest = status?.latest;
  if (!status || !latest) return null;
  const busy = status.state === "downloading" || status.state === "installing";
  if (dismissed === latest.version && !busy) return null;

  const install = async () => {
    setError(null);
    try {
      publishUpdateStatus(await api.installUpdate());
    } catch (e) {
      setError(String(e));
    }
  };

  const openPage = async () => {
    setError(null);
    try {
      await api.openReleasePage();
    } catch (e) {
      setError(String(e));
    }
  };

  const version = `v${latest.version}`;
  const pct = downloadPercent(status);
  // A failed download keeps the release, so the same button retries.
  const failed = status.state === "error" ? (status.message ?? error) : error;

  let text: string;
  if (status.state === "downloading") {
    text = `Downloading ProjectTrucks ${version}${pct === null ? "…" : ` (${pct}%)`}`;
  } else if (status.state === "installing") {
    text = `Installing ProjectTrucks ${version}. The app will close and reopen when it's done.`;
  } else {
    text = `ProjectTrucks ${version} is available. You have v${status.currentVersion}.`;
  }

  return (
    <section
      className="flex flex-wrap items-center gap-3 border-b bg-primary/10 px-6 py-3"
      aria-label="App update"
      data-testid="update-banner"
    >
      <p className="flex-1 text-sm" role="status">
        {text}
      </p>
      {!busy &&
        (latest.canInstall ? (
          <Button type="button" size="sm" onClick={install}>
            {failed ? "Try again" : `Update to ${version}`}
          </Button>
        ) : (
          <Button type="button" size="sm" onClick={openPage}>
            Update available
          </Button>
        ))}
      {!busy && (
        <Button type="button" size="sm" variant="ghost" onClick={() => setDismissed(latest.version)}>
          Later
        </Button>
      )}
      {failed && !busy && (
        <p className="w-full text-sm text-destructive" role="alert" data-testid="update-error">
          {failed}
        </p>
      )}
    </section>
  );
}
