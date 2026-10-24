import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import { updateSettings, useSettings } from "@/lib/settings";
import { useTelemetryStatus, type TelemetryStatus } from "@/lib/useTelemetryEvents";
import type { PluginState, PluginStatus, SettingsPatch } from "@/types";

const STATUS_LABEL: Record<TelemetryStatus, string> = {
  not_connected: "Not connected",
  parked: "Connected, parked",
  free_roam: "Connected, free roam",
  delivery: "Connected, on a delivery",
};

const PLUGIN_LABEL: Record<PluginState, string> = {
  noFolder: "Set the ETS2 folder first",
  badFolder: "Not an ETS2 folder",
  notInstalled: "Not installed",
  outdated: "Outdated",
  installed: "Installed",
};

/** What the plugin check found for one folder; `error` when the check itself failed. */
interface PluginCheck {
  folder: string | null;
  status: PluginStatus | null;
  error: string | null;
}

export function Settings() {
  const settings = useSettings();
  const status = useTelemetryStatus();
  // The path field is edited freely and saved on blur or Enter; `null` = showing the saved value.
  const [draftPath, setDraftPath] = useState<string | null>(null);

  const folder = settings?.ets2InstallPath ?? null;
  const [check, setCheck] = useState<PluginCheck | null>(null);
  const [installing, setInstalling] = useState(false);
  // Which data action is running, so its button is disabled while the file is written.
  const [busy, setBusy] = useState<"export" | "backup" | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .telemetryPluginStatus()
      .then((status) => !cancelled && setCheck({ folder, status, error: null }))
      .catch((e) => !cancelled && setCheck({ folder, status: null, error: String(e) }));
    return () => {
      cancelled = true;
    };
  }, [folder]);

  if (!settings) return null;

  // A result for a folder that is no longer the saved one is stale: show "Checking…" instead.
  const current = check && check.folder === folder ? check : null;
  const pluginState = current?.status?.state;
  const canInstall =
    !installing &&
    (pluginState === "notInstalled" || pluginState === "outdated");

  const installPlugin = async () => {
    setInstalling(true);
    try {
      const status = await api.installTelemetryPlugin();
      setCheck({ folder, status, error: null });
      toast.success("Telemetry plugin installed. Restart ETS2 if it is running.");
    } catch (e) {
      setCheck({ folder, status: null, error: String(e) });
      toast.error(String(e));
    } finally {
      setInstalling(false);
    }
  };

  const save = async (patch: SettingsPatch) => {
    try {
      await updateSettings(patch);
    } catch (e) {
      toast.error(`Could not save settings: ${String(e)}`);
    }
  };

  const savePath = async () => {
    if (draftPath === null) return;
    const value = draftPath.trim();
    setDraftPath(null);
    if (value !== (settings.ets2InstallPath ?? "")) await save({ ets2InstallPath: value });
  };

  const today = new Date().toISOString().slice(0, 10);

  const exportCsv = async () => {
    setBusy("export");
    try {
      const path = await api.pickSavePath({
        defaultPath: `projecttrucks-jobs-${today}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!path) return;
      const count = await api.exportJobsCsv(path);
      toast.success(`Exported ${count} ${count === 1 ? "job" : "jobs"} to ${path}`);
    } catch (e) {
      toast.error(`Export failed: ${String(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const backup = async () => {
    setBusy("backup");
    try {
      const path = await api.pickSavePath({
        defaultPath: `projecttrucks-backup-${today}.db`,
        filters: [{ name: "SQLite database", extensions: ["db"] }],
      });
      if (!path) return;
      await api.backupDatabase(path);
      toast.success(`Backup saved to ${path}`);
    } catch (e) {
      toast.error(`Backup failed: ${String(e)}`);
    } finally {
      setBusy(null);
    }
  };

  const detect = async () => {
    try {
      const found = await api.detectEts2();
      if (!found) {
        toast.info("Could not find ETS2 in your Steam libraries. Enter the folder by hand.");
        return;
      }
      setDraftPath(null);
      await save({ ets2InstallPath: found });
    } catch (e) {
      toast.error(`Detecting ETS2 failed: ${String(e)}`);
    }
  };

  return (
    <div className="max-w-2xl space-y-8">
      <h2 className="text-2xl font-semibold tracking-tight">Settings</h2>

      <section className="space-y-3" aria-labelledby="auto-capture-heading">
        <h3 id="auto-capture-heading" className="text-lg font-medium">
          Auto-capture
        </h3>
        <div className="flex items-center gap-3">
          <Checkbox
            id="auto-capture"
            checked={settings.autoCapture}
            onCheckedChange={(checked) => save({ autoCapture: checked === true })}
          />
          <Label htmlFor="auto-capture">Record drives and deliveries from ETS2 automatically</Label>
        </div>
        <p className="text-sm text-muted-foreground">
          Needs the telemetry plugin in the game. Turning it off ends any drive being recorded.
        </p>
        <div className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground">Telemetry:</span>
          <Badge variant="outline" data-testid="telemetry-badge">
            {STATUS_LABEL[status]}
          </Badge>
        </div>
      </section>

      <section className="space-y-3" aria-labelledby="path-heading">
        <h3 id="path-heading" className="text-lg font-medium">
          ETS2 folder
        </h3>
        <div className="flex gap-2">
          <Input
            aria-label="ETS2 install folder"
            placeholder="Not set"
            value={draftPath ?? settings.ets2InstallPath ?? ""}
            onChange={(e) => setDraftPath(e.target.value)}
            onBlur={savePath}
            onKeyDown={(e) => {
              if (e.key === "Enter") void savePath();
            }}
          />
          <Button type="button" variant="outline" onClick={detect}>
            Detect
          </Button>
        </div>
        <p className="text-sm text-muted-foreground">
          Found through Steam by default. The telemetry plugin goes into this folder.
        </p>
      </section>

      <section className="space-y-3" aria-labelledby="plugin-heading">
        <h3 id="plugin-heading" className="text-lg font-medium">
          Telemetry plugin
        </h3>
        <div className="flex items-center gap-3 text-sm">
          <span className="text-muted-foreground">Status:</span>
          <Badge variant="outline" data-testid="plugin-badge">
            {current
              ? current.error
                ? "Could not check"
                : PLUGIN_LABEL[current.status!.state]
              : "Checking…"}
          </Badge>
          <Button type="button" variant="outline" disabled={!canInstall} onClick={installPlugin}>
            {pluginState === "outdated" ? "Update plugin" : "Install plugin"}
          </Button>
        </div>
        {current?.error && (
          <p className="text-sm text-destructive" role="alert" data-testid="plugin-error">
            {current.error}
          </p>
        )}
        <p className="text-sm text-muted-foreground">
          Copies the bundled plugin into the ETS2 folder (bin/win_x64/plugins). A different plugin
          already there is kept as a .bak file. Restart the game afterwards.
        </p>
      </section>

      <section className="space-y-3" aria-labelledby="units-heading">
        <h3 id="units-heading" className="text-lg font-medium">
          Units
        </h3>
        <div className="flex gap-2" role="group" aria-label="Distance unit">
          {(["km", "mi"] as const).map((unit) => (
            <Button
              key={unit}
              type="button"
              variant={settings.units === unit ? "default" : "outline"}
              aria-pressed={settings.units === unit}
              onClick={() => save({ units: unit })}
            >
              {unit === "km" ? "Kilometres" : "Miles"}
            </Button>
          ))}
        </div>
        <p className="text-sm text-muted-foreground">
          Distances are stored in kilometres; this only changes how they are shown.
        </p>
      </section>

      <section className="space-y-3" aria-labelledby="data-heading">
        <h3 id="data-heading" className="text-lg font-medium">
          Your data
        </h3>
        <div className="flex flex-wrap gap-2">
          <Button type="button" variant="outline" disabled={busy !== null} onClick={exportCsv}>
            Export jobs to CSV
          </Button>
          <Button type="button" variant="outline" disabled={busy !== null} onClick={backup}>
            Back up database
          </Button>
        </div>
        <p className="text-sm text-muted-foreground">
          The CSV has one row per job for spreadsheets. The backup is a copy of the whole SQLite
          database (jobs, drives, trucks, settings); to restore it, close ProjectTrucks and copy
          the file over projecttrucks.db in the app data folder.
        </p>
      </section>
    </div>
  );
}
