import { describe, expect, it } from "vitest";

import { trackingState } from "./tracking";

describe("trackingState", () => {
  it("is off whenever auto-capture is off, whatever the game says", () => {
    expect(trackingState(false, "not_connected")).toBe("off");
    expect(trackingState(false, "delivery")).toBe("off");
  });

  it("waits for the game when auto-capture is on but nothing is connected", () => {
    expect(trackingState(true, "not_connected")).toBe("waiting");
  });

  it("tracks once the game sends frames", () => {
    expect(trackingState(true, "parked")).toBe("tracking");
    expect(trackingState(true, "free_roam")).toBe("tracking");
    expect(trackingState(true, "delivery")).toBe("tracking");
  });
});
