import { Link, useParams } from "react-router-dom";
import { ArrowLeft } from "lucide-react";

import { Button } from "@/components/ui/button";

export function JobDetail() {
  const { id } = useParams<{ id: string }>();
  return (
    <div className="space-y-4">
      <Button variant="ghost" size="sm" asChild>
        <Link to="/jobs">
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back to jobs
        </Link>
      </Button>
      <div className="space-y-2">
        <h2 className="text-2xl font-semibold tracking-tight">Job detail</h2>
        <p className="text-muted-foreground text-sm">
          Job id: <code className="font-mono text-xs">{id}</code>
        </p>
        <p className="text-muted-foreground text-sm">
          Full record, editable notes, and a delete button land in Task 1.6.
        </p>
      </div>
    </div>
  );
}
