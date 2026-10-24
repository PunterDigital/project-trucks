import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";

import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  /** What "Reload" does; defaults to reloading the webview. Tests pass a spy. */
  onReload?: () => void;
}

interface State {
  error: Error | null;
}

/**
 * The app-wide last resort: a render error anywhere in the tree swaps the whole window
 * for this screen instead of leaving it blank. Reload restarts the frontend; the Rust
 * side (and the database) is untouched, so nothing is lost.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("Unhandled render error", error, info.componentStack);
  }

  private reload = (): void => {
    if (this.props.onReload) this.props.onReload();
    else window.location.reload();
  };

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div
        role="alert"
        data-testid="app-crash"
        className="bg-background text-foreground flex h-screen items-center justify-center p-6"
      >
        <div className="w-full max-w-md space-y-4 text-center">
          <AlertTriangle className="text-destructive mx-auto h-10 w-10" aria-hidden="true" />
          <div className="space-y-1">
            <h1 className="text-xl font-semibold tracking-tight">Something went wrong</h1>
            <p className="text-muted-foreground text-sm">
              ProjectTrucks hit an error it couldn't recover from. Your jobs and settings are
              safe in the database; reloading starts the window again.
            </p>
          </div>
          <pre className="bg-muted text-muted-foreground max-h-40 overflow-auto rounded-md p-3 text-left text-xs whitespace-pre-wrap break-words">
            {error.message || String(error)}
          </pre>
          <Button onClick={this.reload}>Reload</Button>
        </div>
      </div>
    );
  }
}
