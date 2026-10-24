// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { ErrorBoundary } from "./ErrorBoundary";

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.restoreAllMocks();
});

function Boom({ when }: { when: boolean }) {
  if (when) throw new Error("kaboom: the chart ate a null");
  return <p>all fine</p>;
}

test("renders its children while nothing throws", async () => {
  await act(async () => {
    root.render(
      <ErrorBoundary>
        <Boom when={false} />
      </ErrorBoundary>,
    );
  });
  expect(container.textContent).toBe("all fine");
  expect(container.querySelector('[data-testid="app-crash"]')).toBeNull();
});

test("a render error shows the crash screen with the message and a Reload action", async () => {
  // React and the boundary both log the error; keep the test output clean.
  vi.spyOn(console, "error").mockImplementation(() => {});
  const reload = vi.fn();

  await act(async () => {
    root.render(
      <ErrorBoundary onReload={reload}>
        <Boom when={true} />
      </ErrorBoundary>,
    );
  });

  const crash = container.querySelector('[data-testid="app-crash"]');
  expect(crash).not.toBeNull();
  expect(crash?.getAttribute("role")).toBe("alert");
  expect(crash?.textContent).toContain("Something went wrong");
  expect(crash?.textContent).toContain("kaboom: the chart ate a null");

  const button = Array.from(container.querySelectorAll("button")).find(
    (b) => b.textContent === "Reload",
  );
  expect(button).toBeDefined();
  await act(async () => {
    button!.click();
  });
  expect(reload).toHaveBeenCalledTimes(1);
});
