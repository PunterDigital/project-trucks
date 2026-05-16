import { Link } from "react-router-dom";
import { Button } from "@/components/ui/button";

export function Jobs() {
  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-2">
          <h2 className="text-2xl font-semibold tracking-tight">Jobs</h2>
          <p className="text-muted-foreground">
            Searchable, paginated delivery history will live here.
          </p>
        </div>
        <Button asChild>
          <Link to="/jobs/new">Log a job</Link>
        </Button>
      </div>
    </div>
  );
}
