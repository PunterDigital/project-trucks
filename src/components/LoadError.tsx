import { AlertTriangle } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/**
 * A page or list whose data failed to load. Shown in place of the data (a toast alone
 * would leave the page blank), with the backend's message and a retry.
 */
export function LoadError({
  what,
  message,
  onRetry,
  compact = false,
  className,
}: {
  /** What failed, e.g. "the dashboard" or "this truck's jobs". */
  what: string;
  message: string;
  onRetry?: () => void;
  compact?: boolean;
  className?: string;
}) {
  return (
    <div
      role="alert"
      data-testid="load-error"
      className={cn(
        "border-destructive/40 bg-destructive/5 rounded-md border text-center",
        compact ? "p-6" : "p-10",
        className,
      )}
    >
      <AlertTriangle
        className={cn("text-destructive mx-auto mb-3", compact ? "h-6 w-6" : "h-8 w-8")}
        aria-hidden="true"
      />
      <h3 className={cn("font-medium", compact ? "text-base" : "text-lg")}>
        Couldn't load {what}
      </h3>
      <p className="text-muted-foreground mx-auto mt-1 max-w-md break-words text-sm">{message}</p>
      {onRetry && (
        <div className="mt-4">
          <Button variant="outline" size="sm" onClick={onRetry}>
            Try again
          </Button>
        </div>
      )}
    </div>
  );
}
