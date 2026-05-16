import { Link, useParams } from "react-router-dom";
import { ArrowLeft } from "lucide-react";

import { Button } from "@/components/ui/button";

export function TruckDetail() {
  const { id } = useParams<{ id: string }>();
  return (
    <div className="space-y-4">
      <Button variant="ghost" size="sm" asChild>
        <Link to="/trucks">
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back to trucks
        </Link>
      </Button>
      <div className="space-y-2">
        <h2 className="text-2xl font-semibold tracking-tight">Truck detail</h2>
        <p className="text-muted-foreground text-sm">
          Truck id: <code className="font-mono text-xs">{id}</code>
        </p>
        <p className="text-muted-foreground text-sm">
          Per-truck stats, job list, and distance chart land in Task 3.2.
        </p>
      </div>
    </div>
  );
}
