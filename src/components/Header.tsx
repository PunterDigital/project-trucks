import { Badge } from "@/components/ui/badge";
import { useSettings } from "@/lib/settings";

export function Header() {
  const settings = useSettings();
  const on = settings?.autoCapture ?? false;
  return (
    <header className="flex h-14 items-center justify-between border-b px-6">
      <h1 className="text-base font-semibold tracking-tight">ProjectTrucks</h1>
      <Badge
        variant={on ? "default" : "secondary"}
        className="font-normal"
        data-testid="auto-capture-pill"
      >
        Auto-capture: {on ? "on" : "off"}
      </Badge>
    </header>
  );
}
