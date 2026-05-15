import { Badge } from "@/components/ui/badge";

export function Header() {
  return (
    <header className="flex h-14 items-center justify-between border-b px-6">
      <h1 className="text-base font-semibold tracking-tight">ProjectTrucks</h1>
      <Badge variant="secondary" className="font-normal">
        Auto-capture: off
      </Badge>
    </header>
  );
}
