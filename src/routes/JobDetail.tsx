import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import { ArrowLeft, Loader2 } from "lucide-react";

import { JobStatusBadge } from "@/components/JobStatusBadge";
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";

import { api } from "@/lib/api";
import {
  formatCurrency,
  formatDate,
  formatDuration,
  formatKm,
} from "@/lib/format";
import type { Job } from "@/types";

type Status = "idle" | "pending" | "saving" | "saved" | "error";
type LoadState =
  | { kind: "loading" }
  | { kind: "missing" }
  | { kind: "ready"; job: Job }
  | { kind: "error"; message: string };

export function JobDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();

  const [load, setLoad] = useState<LoadState>({ kind: "loading" });
  const [notes, setNotes] = useState("");
  const [flagged, setFlagged] = useState(false);
  const [flagReason, setFlagReason] = useState("");
  const [saveStatus, setSaveStatus] = useState<Status>("idle");

  // What's currently on the server, so we can detect dirty state and bypass
  // the save loop on initial hydration.
  const lastSavedRef = useRef<{
    notes: string;
    flagged: boolean;
    flagReason: string;
  } | null>(null);

  // Load
  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    setLoad({ kind: "loading" });
    api
      .getJob(id)
      .then((j) => {
        if (cancelled) return;
        if (!j) {
          setLoad({ kind: "missing" });
          return;
        }
        setLoad({ kind: "ready", job: j });
        setNotes(j.notes ?? "");
        setFlagged(j.flagged);
        setFlagReason(j.flagReason ?? "");
        lastSavedRef.current = {
          notes: j.notes ?? "",
          flagged: j.flagged,
          flagReason: j.flagReason ?? "",
        };
      })
      .catch((e: unknown) => {
        if (!cancelled) setLoad({ kind: "error", message: String(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [id]);

  // Debounced save of the three editable fields.
  useEffect(() => {
    if (!id || !lastSavedRef.current) return;
    const current = { notes, flagged, flagReason };
    const ref = lastSavedRef.current;
    if (
      current.notes === ref.notes &&
      current.flagged === ref.flagged &&
      current.flagReason === ref.flagReason
    ) {
      return;
    }
    setSaveStatus("pending");
    const timer = setTimeout(() => {
      setSaveStatus("saving");
      api
        .updateJob(id, current)
        .then((updated) => {
          if (updated) {
            lastSavedRef.current = current;
            setLoad({ kind: "ready", job: updated });
          }
          setSaveStatus("saved");
          window.setTimeout(() => setSaveStatus("idle"), 1500);
        })
        .catch((e: unknown) => {
          setSaveStatus("error");
          toast.error(`Couldn't save: ${String(e)}`);
        });
    }, 800);
    return () => clearTimeout(timer);
  }, [notes, flagged, flagReason, id]);

  const handleDelete = async () => {
    if (!id) return;
    try {
      await api.deleteJob(id);
      toast.success("Job deleted");
      navigate("/jobs");
    } catch (e) {
      toast.error(`Couldn't delete: ${String(e)}`);
    }
  };

  if (load.kind === "loading") {
    return (
      <div className="space-y-4">
        <BackLink />
        <p className="text-muted-foreground text-sm">Loading…</p>
      </div>
    );
  }

  if (load.kind === "missing") {
    return (
      <div className="space-y-4">
        <BackLink />
        <div className="rounded-md border border-dashed p-10 text-center">
          <h2 className="text-lg font-medium">Job not found</h2>
          <p className="text-muted-foreground mt-1 text-sm">
            It may have been deleted, or the id in the URL is mistyped.
          </p>
        </div>
      </div>
    );
  }

  if (load.kind === "error") {
    return (
      <div className="space-y-4">
        <BackLink />
        <p className="text-destructive text-sm">Error: {load.message}</p>
      </div>
    );
  }

  const job = load.job;

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div className="flex items-center justify-between gap-4">
        <BackLink />
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="destructive">Delete job</Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Delete this job?</AlertDialogTitle>
              <AlertDialogDescription>
                The job row is removed and the truck's rollup is recomputed.
                The underlying driving session (if any) survives — the
                kilometres still happened.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={handleDelete}
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              >
                Delete
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>

      {/* Hero */}
      <header className="space-y-3">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            {job.sourceCity} → {job.destCity}
          </h1>
          <span className="text-muted-foreground text-sm">
            {job.sourceCompany} → {job.destCompany}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <JobStatusBadge status={job.status} />
          <Badge variant="outline">
            {job.source === "MANUAL" ? "Manual entry" : "Telemetry"}
          </Badge>
          {job.flagged && (
            <Badge className="border-transparent bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300">
              Flagged
            </Badge>
          )}
        </div>
      </header>

      {/* Big-number summary */}
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <Stat label="Distance" value={formatKm(job.distanceKm)} />
        <Stat label="Duration" value={formatDuration(job.durationSec)} />
        <Stat
          label="Avg speed"
          value={`${job.avgSpeedKmh.toFixed(1)} km/h`}
        />
        <Stat label="Income" value={formatCurrency(job.declaredIncome)} />
      </div>

      <Separator />

      {/* Details */}
      <Section title="Cargo">
        <Row label="Cargo id" value={<code className="font-mono text-xs">{job.cargoId}</code>} />
        <Row label="Cargo name" value={job.cargoName} />
        <Row label="Mass" value={`${job.cargoMassKg.toLocaleString()} kg`} />
      </Section>

      <Section title="Trip">
        <Row label="Started at" value={formatDate(job.startedAt) + ", " + new Date(job.startedAt).toLocaleTimeString()} />
        <Row label="Completed at" value={formatDate(job.completedAt) + ", " + new Date(job.completedAt).toLocaleTimeString()} />
        <Row
          label="Truck"
          value={
            job.truckMake && job.truckModel
              ? `${job.truckMake} ${job.truckModel}`
              : "—"
          }
        />
      </Section>

      <Section title="Diagnostic">
        <Row
          label="Fuel used"
          value={job.fuelUsedL != null ? `${job.fuelUsedL} L` : "—"}
        />
        <Row
          label="Damage delta"
          value={
            job.damageDelta != null
              ? `${(job.damageDelta * 100).toFixed(1)}%`
              : "—"
          }
        />
        <Row
          label="Session"
          value={
            job.sessionId
              ? <code className="font-mono text-xs">{job.sessionId}</code>
              : "—"
          }
        />
      </Section>

      <Separator />

      {/* Review (editable) */}
      <Section
        title="Review"
        right={<SaveIndicator status={saveStatus} />}
      >
        <div className="space-y-3">
          <label className="text-sm font-medium">Notes</label>
          <Textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            placeholder="What went well, what didn't, anything worth remembering…"
            rows={4}
          />
        </div>

        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <Checkbox
              id="flagged"
              checked={flagged}
              onCheckedChange={(v) => setFlagged(v === true)}
            />
            <label htmlFor="flagged" className="text-sm font-medium select-none">
              Flag this job for review
            </label>
          </div>
          {flagged && (
            <Input
              value={flagReason}
              onChange={(e) => setFlagReason(e.target.value)}
              placeholder="Why is this flagged? (optional)"
            />
          )}
        </div>
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Bits
// ---------------------------------------------------------------------------

function BackLink() {
  return (
    <Button variant="ghost" size="sm" asChild>
      <Link to="/jobs">
        <ArrowLeft className="mr-2 h-4 w-4" />
        Back to jobs
      </Link>
    </Button>
  );
}

function Stat({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <div className="text-muted-foreground text-xs uppercase tracking-wide">
        {label}
      </div>
      <div className="text-xl font-semibold tabular-nums">{value}</div>
    </div>
  );
}

function Section({
  title,
  right,
  children,
}: {
  title: string;
  right?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-3">
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-sm font-medium uppercase tracking-wide text-muted-foreground">
          {title}
        </h3>
        {right}
      </div>
      <div className="space-y-2">{children}</div>
    </section>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="grid grid-cols-3 items-baseline gap-3 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="col-span-2">{value}</span>
    </div>
  );
}

function SaveIndicator({ status }: { status: Status }) {
  if (status === "idle") return null;
  if (status === "pending")
    return (
      <span className="text-muted-foreground text-xs">Unsaved changes…</span>
    );
  if (status === "saving")
    return (
      <span className="text-muted-foreground flex items-center gap-1 text-xs">
        <Loader2 className="h-3 w-3 animate-spin" />
        Saving…
      </span>
    );
  if (status === "saved")
    return <span className="text-xs text-emerald-600 dark:text-emerald-400">Saved</span>;
  return <span className="text-destructive text-xs">Save failed</span>;
}
