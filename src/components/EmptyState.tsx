import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/**
 * The dashed "nothing here yet" box every list and page uses. `children` holds the
 * next-action buttons; `compact` is the smaller variant for a list inside a page
 * (a truck's jobs tab) where the page already has its own heading.
 */
export function EmptyState({
  icon: Icon,
  title,
  description,
  children,
  compact = false,
  className,
  ...props
}: {
  icon?: LucideIcon;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  compact?: boolean;
} & Omit<React.ComponentProps<"div">, "title">) {
  return (
    <div
      data-testid="empty-state"
      className={cn(
        "rounded-md border border-dashed text-center",
        compact ? "p-6" : "p-10",
        className,
      )}
      {...props}
    >
      {Icon && (
        <Icon
          className={cn("text-muted-foreground mx-auto mb-3", compact ? "h-6 w-6" : "h-8 w-8")}
          aria-hidden="true"
        />
      )}
      <h3 className={cn("font-medium", compact ? "text-base" : "text-lg")}>{title}</h3>
      {description && (
        <p className="text-muted-foreground mx-auto mt-1 max-w-md text-sm">{description}</p>
      )}
      {children && <div className="mt-4 flex flex-wrap justify-center gap-2">{children}</div>}
    </div>
  );
}
