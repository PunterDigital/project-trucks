import { useState } from "react";
import { toast } from "sonner";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { publishHubStatus, syncSummary, useHubStatus } from "@/lib/hub";
import type { HubStatus } from "@/types";

function when(iso: string | null): string {
  if (!iso) return "never";
  return new Date(iso).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Settings → ProjectTrucks Hub: link the app to an account and see the sync. */
export function HubSettings() {
  const status = useHubStatus();
  const [busy, setBusy] = useState(false);

  const run = async (action: () => Promise<HubStatus>, done?: (s: HubStatus) => void) => {
    setBusy(true);
    try {
      const next = await action();
      publishHubStatus(next);
      done?.(next);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const unlink = () => run(api.hubUnlink, () => toast.success("Unlinked from the Hub."));

  const open = (target: "approve" | "profile") =>
    api.hubOpen(target).catch((e) => toast.error(String(e)));

  return (
    <section className="space-y-3" aria-labelledby="hub-heading" data-testid="hub-settings">
      <h3 id="hub-heading" className="text-lg font-medium">
        ProjectTrucks Hub
      </h3>

      {!status ? null : status.hubUrl === null ? (
        <p className="text-sm text-muted-foreground" data-testid="hub-off">
          The Hub is off in this profile.
        </p>
      ) : status.linked && status.account ? (
        <div className="space-y-3" data-testid="hub-linked">
          <div className="flex flex-wrap items-center gap-3 text-sm">
            <span>
              Linked as <strong>{status.account.name}</strong>
              {status.account.username && (
                <span className="text-muted-foreground"> (@{status.account.username})</span>
              )}
            </span>
            <Button type="button" variant="outline" size="sm" onClick={() => open("profile")}>
              Open my profile
            </Button>
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button type="button" variant="outline" size="sm" disabled={busy}>
                  Unlink
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Unlink from the Hub?</AlertDialogTitle>
                  <AlertDialogDescription>
                    New jobs stop syncing and the website stops showing you on the road. Jobs
                    already on the Hub stay there; you can hide them on the website.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Cancel</AlertDialogCancel>
                  <AlertDialogAction onClick={() => void unlink()}>Unlink</AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </div>
          <p className="text-sm" data-testid="hub-sync-summary">
            {syncSummary(status)}
            <span className="text-muted-foreground"> · last sync {when(status.lastSyncAt)}</span>
          </p>
          <div className="flex flex-wrap gap-2">
            {status.history > 0 && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() =>
                  run(api.hubSyncHistory, () =>
                    toast.success("Older jobs are syncing to the Hub."),
                  )
                }
              >
                Sync history ({status.history} older {status.history === 1 ? "job" : "jobs"})
              </Button>
            )}
            {(status.failed > 0 || status.lastError) && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={busy}
                onClick={() => run(api.hubSyncNow)}
              >
                Retry now
              </Button>
            )}
          </div>
        </div>
      ) : status.linking ? (
        <div className="space-y-3" data-testid="hub-linking">
          <p className="text-sm">
            Approve this code on the Hub website. It opened in your browser; if not, click{" "}
            <strong>Open approval page</strong>.
          </p>
          <p
            className="font-mono text-3xl font-semibold tracking-widest"
            data-testid="hub-code"
          >
            {status.linking.userCode}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button type="button" onClick={() => open("approve")}>
              Open approval page
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => run(api.hubLinkCancel)}
            >
              Cancel
            </Button>
          </div>
          <p className="text-sm text-muted-foreground">
            Waiting for approval… The code works until {when(status.linking.expiresAt)}.
          </p>
        </div>
      ) : (
        <div className="space-y-3" data-testid="hub-unlinked">
          <Button type="button" disabled={busy} onClick={() => run(api.hubLinkStart)}>
            Link account
          </Button>
        </div>
      )}

      {status?.lastError && (
        <p className="text-sm text-destructive" role="alert" data-testid="hub-error">
          {status.lastError}
        </p>
      )}

      <p className="text-sm text-muted-foreground">
        Share your jobs on the ProjectTrucks Hub: your profile, a company and the leaderboards.
        Nothing leaves this PC until you link an account. Once linked, new jobs sync by
        themselves and show on your public profile, along with what you&apos;re driving right
        now; older jobs only sync if you click Sync history. Job notes always stay on this PC.
        You can hide jobs or make your profile private on the website.
      </p>
    </section>
  );
}
