import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { updateSettings, useSettings } from "@/lib/settings";
import type { PluginStatus } from "@/types";

/** What the plugin check found for one folder; `null` status when the check failed. */
interface Check {
  folder: string | null;
  status: PluginStatus | null;
}

/**
 * Suggests installing the telemetry plugin when it is missing or outdated. "Not now" is
 * remembered per bundled plugin version, so it only comes back when the app ships a new plugin.
 */
export function PluginPrompt() {
  const settings = useSettings();
  const { pathname } = useLocation();
  const folder = settings?.ets2InstallPath ?? null;
  const [check, setCheck] = useState<Check | null>(null);
  // Edited freely; `null` = showing the saved folder.
  const [draft, setDraft] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!settings) return;
    let cancelled = false;
    api
      .telemetryPluginStatus()
      .then((status) => !cancelled && setCheck({ folder, status }))
      .catch(() => !cancelled && setCheck({ folder, status: null }));
    return () => {
      cancelled = true;
    };
    // Re-check only when the saved folder changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings !== null, folder]);

  const status = check && check.folder === folder ? check.status : null;
  if (!settings || !status || pathname === "/settings") return null;
  if (status.state === "installed" || !status.bundledSha) return null;
  if (settings.pluginPromptDismissed === status.bundledSha) return null;

  const outdated = status.state === "outdated";
  const shown = draft ?? folder ?? "";

  const detect = async () => {
    try {
      const found = await api.detectEts2();
      if (!found) {
        setError("Could not find ETS2 in your Steam libraries. Enter the folder by hand.");
        return;
      }
      setError(null);
      setDraft(found);
    } catch (e) {
      setError(`Detecting ETS2 failed: ${String(e)}`);
    }
  };

  const install = async () => {
    setBusy(true);
    setError(null);
    try {
      const value = shown.trim();
      if (value !== (folder ?? "")) await updateSettings({ ets2InstallPath: value });
      const next = await api.installTelemetryPlugin();
      setDraft(null);
      setCheck({ folder: value || null, status: next });
      toast.success("Telemetry plugin installed. Restart ETS2 if it is running.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const notNow = async () => {
    try {
      await updateSettings({ pluginPromptDismissed: status.bundledSha! });
    } catch (e) {
      toast.error(`Could not save settings: ${String(e)}`);
    }
  };

  return (
    <section
      className="space-y-3 border-b bg-muted/40 px-6 py-4"
      aria-labelledby="plugin-prompt-heading"
      data-testid="plugin-prompt"
    >
      <div>
        <h2 id="plugin-prompt-heading" className="text-sm font-semibold">
          {outdated ? "Update live telemetry?" : "Set up live telemetry?"}
        </h2>
        <p className="text-sm text-muted-foreground">
          {outdated
            ? "The telemetry plugin in your ETS2 folder is not the one this version ships."
            : "ProjectTrucks records your deliveries through a small plugin in the ETS2 folder."}{" "}
          Point it at the game and it installs the plugin for you.
        </p>
      </div>
      <div className="flex gap-2">
        <Input
          aria-label="ETS2 folder for the plugin"
          placeholder="Not set"
          value={shown}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button type="button" variant="outline" onClick={detect} disabled={busy}>
          Detect folder
        </Button>
        <Button type="button" onClick={install} disabled={busy || shown.trim() === ""}>
          {outdated ? "Update" : "Install"}
        </Button>
        <Button type="button" variant="ghost" onClick={notNow} disabled={busy}>
          Not now
        </Button>
      </div>
      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
