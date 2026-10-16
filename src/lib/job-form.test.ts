import { describe, expect, it } from "vitest";

import { jobFormSchema, toNewJob, type JobFormValues } from "./job-form";

const valid: JobFormValues = {
  sourceCity: " Berlin ",
  sourceCompany: "Posped",
  destCity: "Praha",
  destCompany: "Tradeaux",
  cargoId: "electronics",
  cargoName: "Electronics",
  cargoMassKg: "12000",
  declaredIncome: "4250",
  distanceKm: "350.5",
  durationHours: "4",
  durationMinutes: "30",
  startedAt: "2026-05-16T08:00",
  completedAt: "2026-05-16T12:30",
  status: "DELIVERED",
  truckMake: "Scania",
  truckModel: "S",
  fuelUsedL: "",
  damageDelta: "",
  notes: "",
};

/** Error messages per field path for `values`, or {} when valid. */
function errors(values: Partial<JobFormValues>): Record<string, string> {
  const result = jobFormSchema.safeParse({ ...valid, ...values });
  if (result.success) return {};
  return Object.fromEntries(
    result.error.issues.map((i) => [i.path.join("."), i.message]),
  );
}

describe("jobFormSchema", () => {
  it("accepts a complete job", () => {
    expect(errors({})).toEqual({});
  });

  it("requires the route, cargo and truck text fields", () => {
    expect(errors({ sourceCity: "", truckModel: "" })).toEqual({
      sourceCity: "Required",
      truckModel: "Required",
    });
  });

  it("checks numeric fields", () => {
    expect(errors({ cargoMassKg: "-1" }).cargoMassKg).toBe("Must be a number ≥ 0");
    expect(errors({ cargoMassKg: "" }).cargoMassKg).toBe("Must be a number ≥ 0");
    expect(errors({ cargoMassKg: "0" })).toEqual({});
    expect(errors({ declaredIncome: "10.5" }).declaredIncome).toBe(
      "Whole numbers ≥ 0 only",
    );
    expect(errors({ distanceKm: "0" }).distanceKm).toBe("Must be > 0");
    expect(errors({ distanceKm: "abc" }).distanceKm).toBe("Must be > 0");
  });

  it("limits duration minutes to 0..59 and needs a non-zero duration", () => {
    expect(errors({ durationMinutes: "60" }).durationMinutes).toBe(
      "Whole number 0..59",
    );
    expect(errors({ durationHours: "1.5" }).durationHours).toBe(
      "Whole numbers ≥ 0 only",
    );
    expect(errors({ durationHours: "0", durationMinutes: "0" })).toEqual({
      durationMinutes: "Duration must be > 0",
    });
    expect(errors({ durationHours: "0", durationMinutes: "45" })).toEqual({});
  });

  it("needs completed at to be after started at", () => {
    expect(errors({ completedAt: "2026-05-16T08:00" })).toEqual({
      completedAt: "Must be after started at",
    });
    expect(errors({ completedAt: "2026-05-16T07:59" })).toEqual({
      completedAt: "Must be after started at",
    });
  });

  it("only checks the optional fields when they're filled in", () => {
    expect(errors({ fuelUsedL: "120.5", damageDelta: "0.25" })).toEqual({});
    expect(errors({ fuelUsedL: "-5" })).toEqual({
      fuelUsedL: "Must be a number ≥ 0",
    });
    expect(errors({ damageDelta: "1.5" })).toEqual({
      damageDelta: "Must be between 0 and 1",
    });
  });

  it("rejects an unknown status", () => {
    expect(errors({ status: "LOST" as JobFormValues["status"] })).toHaveProperty(
      "status",
    );
  });
});

describe("toNewJob", () => {
  it("trims text, parses numbers and marks the job as manual", () => {
    const job = toNewJob({
      ...valid,
      fuelUsedL: "120.5",
      damageDelta: "0.25",
      notes: "  rainy  ",
    });
    expect(job).toMatchObject({
      sessionId: null,
      sourceCity: "Berlin",
      cargoMassKg: 12000,
      declaredIncome: 4250,
      distanceKm: 350.5,
      durationSec: 4 * 3600 + 30 * 60,
      fuelUsedL: 120.5,
      damageDelta: 0.25,
      status: "DELIVERED",
      source: "MANUAL",
      notes: "rainy",
    });
  });

  it("stores the local datetime inputs as UTC ISO timestamps", () => {
    const job = toNewJob(valid);
    expect(job.startedAt).toBe(new Date("2026-05-16T08:00").toISOString());
    expect(job.completedAt).toBe(new Date("2026-05-16T12:30").toISOString());
  });

  it("sends null for empty optional fields", () => {
    const job = toNewJob(valid);
    expect(job.fuelUsedL).toBeNull();
    expect(job.damageDelta).toBeNull();
    expect(job.notes).toBeNull();
  });
});
