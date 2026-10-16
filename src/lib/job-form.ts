import * as z from "zod";

import type { NewJob } from "@/types";

// Validation for the manual "Log a job" form (routes/JobNew.tsx).
//
// All editable fields are strings in the form state — easy to render, and we
// avoid the z.coerce.number() input/output type split that confuses
// react-hook-form's TFieldValues vs TTransformedValues. Numeric validation
// happens in refine() and the parse-to-Number happens once in toNewJob().

const nonNegativeInt = (value: string) => {
  if (value.trim() === "") return false;
  const n = Number(value);
  return Number.isFinite(n) && Number.isInteger(n) && n >= 0;
};
const nonNegativeNumber = (value: string) => {
  if (value.trim() === "") return false;
  const n = Number(value);
  return Number.isFinite(n) && n >= 0;
};
const positiveNumber = (value: string) => {
  if (value.trim() === "") return false;
  const n = Number(value);
  return Number.isFinite(n) && n > 0;
};

export const jobFormSchema = z
  .object({
    sourceCity: z.string().min(1, "Required"),
    sourceCompany: z.string().min(1, "Required"),
    destCity: z.string().min(1, "Required"),
    destCompany: z.string().min(1, "Required"),
    cargoId: z.string().min(1, "Required"),
    cargoName: z.string().min(1, "Required"),
    cargoMassKg: z.string().refine(nonNegativeNumber, "Must be a number ≥ 0"),
    declaredIncome: z
      .string()
      .refine(nonNegativeInt, "Whole numbers ≥ 0 only"),
    distanceKm: z.string().refine(positiveNumber, "Must be > 0"),
    durationHours: z.string().refine(nonNegativeInt, "Whole numbers ≥ 0 only"),
    durationMinutes: z.string().refine((v) => {
      if (v.trim() === "") return false;
      const n = Number(v);
      return Number.isFinite(n) && Number.isInteger(n) && n >= 0 && n <= 59;
    }, "Whole number 0..59"),
    startedAt: z.string().min(1, "Required"),
    completedAt: z.string().min(1, "Required"),
    status: z.enum(["DELIVERED", "LATE", "CANCELLED"]),
    truckMake: z.string().min(1, "Required"),
    truckModel: z.string().min(1, "Required"),
    fuelUsedL: z.string().optional(),
    damageDelta: z.string().optional(),
    notes: z.string().optional(),
  })
  .refine(
    (d) => {
      const started = Date.parse(d.startedAt);
      const completed = Date.parse(d.completedAt);
      if (Number.isNaN(started) || Number.isNaN(completed)) return true;
      return completed > started;
    },
    {
      message: "Must be after started at",
      path: ["completedAt"],
    },
  )
  .refine(
    (d) => Number(d.durationHours) > 0 || Number(d.durationMinutes) > 0,
    {
      message: "Duration must be > 0",
      path: ["durationMinutes"],
    },
  )
  .refine(
    (d) => {
      if (!d.fuelUsedL) return true;
      const n = Number(d.fuelUsedL);
      return Number.isFinite(n) && n >= 0;
    },
    {
      message: "Must be a number ≥ 0",
      path: ["fuelUsedL"],
    },
  )
  .refine(
    (d) => {
      if (!d.damageDelta) return true;
      const n = Number(d.damageDelta);
      return Number.isFinite(n) && n >= 0 && n <= 1;
    },
    {
      message: "Must be between 0 and 1",
      path: ["damageDelta"],
    },
  );

export type JobFormValues = z.infer<typeof jobFormSchema>;

/** Turns validated form values into the `create_job` payload. */
export function toNewJob(data: JobFormValues): NewJob {
  const durationSec =
    Number(data.durationHours) * 3600 + Number(data.durationMinutes) * 60;
  return {
    sessionId: null,
    truckMake: data.truckMake.trim(),
    truckModel: data.truckModel.trim(),
    sourceCity: data.sourceCity.trim(),
    sourceCompany: data.sourceCompany.trim(),
    destCity: data.destCity.trim(),
    destCompany: data.destCompany.trim(),
    cargoId: data.cargoId.trim(),
    cargoName: data.cargoName.trim(),
    cargoMassKg: Number(data.cargoMassKg),
    declaredIncome: Number(data.declaredIncome),
    distanceKm: Number(data.distanceKm),
    durationSec,
    fuelUsedL: data.fuelUsedL ? Number(data.fuelUsedL) : null,
    damageDelta: data.damageDelta ? Number(data.damageDelta) : null,
    startedAt: new Date(data.startedAt).toISOString(),
    completedAt: new Date(data.completedAt).toISOString(),
    status: data.status,
    source: "MANUAL",
    notes: data.notes ? data.notes.trim() : null,
  };
}
