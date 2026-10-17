import { useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import { updateSettings, useSettings } from "@/lib/settings";
import { useTelemetryStatus, type TelemetryStatus } from "@/lib/useTelemetryEvents";
import type { SettingsPatch } from "@/types";

const STATUS_LABEL: Record<TelemetryStatus, string> = {
  not_connected: "Not connected",
  parked: "Connected, parked",
  free_roam: "Connected, free roam",
  delivery: "Connected, on a delivery",
};

export function Settings() {
  const settings = useSettings();
  const status = useTelemetryStatus();
  // The path field is edited freely and saved on blur or Enter; `null` = showing the saved value.
  const [draftPath, setDraftPath] = useState<string | null>(null);

  if (!settings) return null;

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
    </div>
  );
}
