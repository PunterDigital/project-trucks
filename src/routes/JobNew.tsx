import { Controller, useForm, type Control } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSeparator,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";

import { api } from "@/lib/api";
import { jobFormSchema, toNewJob, type JobFormValues } from "@/lib/job-form";
import type { JobStatus } from "@/types";


// datetime-local needs "YYYY-MM-DDTHH:MM" without timezone or seconds.
function toLocalInputValue(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function defaultStartedAt(): string {
  return toLocalInputValue(new Date(Date.now() - 60 * 60 * 1000));
}
function defaultCompletedAt(): string {
  return toLocalInputValue(new Date());
}

const STATUS_OPTIONS: { value: JobStatus; label: string }[] = [
  { value: "DELIVERED", label: "Delivered" },
  { value: "LATE", label: "Late" },
  { value: "CANCELLED", label: "Cancelled" },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function JobNew() {
  const navigate = useNavigate();

  const form = useForm<JobFormValues>({
    resolver: zodResolver(jobFormSchema),
    defaultValues: {
      sourceCity: "",
      sourceCompany: "",
      destCity: "",
      destCompany: "",
      cargoId: "",
      cargoName: "",
      cargoMassKg: "0",
      declaredIncome: "0",
      distanceKm: "0",
      durationHours: "1",
      durationMinutes: "0",
      startedAt: defaultStartedAt(),
      completedAt: defaultCompletedAt(),
      status: "DELIVERED",
      truckMake: "",
      truckModel: "",
      fuelUsedL: "",
      damageDelta: "",
      notes: "",
    },
  });

  const onSubmit = async (data: JobFormValues) => {
    const input = toNewJob(data);

    try {
      await api.createJob(input);
      toast.success("Job logged");
      navigate("/jobs");
    } catch (e) {
      toast.error(`Couldn't create job: ${String(e)}`);
    }
  };

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div className="space-y-1">
        <h2 className="text-2xl font-semibold tracking-tight">Log a job</h2>
        <p className="text-muted-foreground">
          Manual entry for deliveries the telemetry missed.
        </p>
      </div>

      <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-8">
        {/* ---- Route ---- */}
        <FieldSet>
          <FieldLegend>Route</FieldLegend>
          <FieldGroup className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FormText name="sourceCity" label="Source city" control={form.control} />
            <FormText
              name="sourceCompany"
              label="Source company"
              control={form.control}
            />
            <FormText name="destCity" label="Destination city" control={form.control} />
            <FormText
              name="destCompany"
              label="Destination company"
              control={form.control}
            />
          </FieldGroup>
        </FieldSet>

        <FieldSeparator />

        {/* ---- Cargo ---- */}
        <FieldSet>
          <FieldLegend>Cargo</FieldLegend>
          <FieldGroup className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FormText
              name="cargoId"
              label="Cargo id"
              description="ETS2's internal cargo identifier, e.g. 'container_red'."
              control={form.control}
            />
            <FormText name="cargoName" label="Cargo name" control={form.control} />
            <FormNumber
              name="cargoMassKg"
              label="Cargo mass (kg)"
              step="100"
              control={form.control}
            />
            <FormNumber
              name="declaredIncome"
              label="Declared income"
              description="In-game currency."
              step="1"
              control={form.control}
            />
          </FieldGroup>
        </FieldSet>

        <FieldSeparator />

        {/* ---- Trip ---- */}
        <FieldSet>
          <FieldLegend>Trip</FieldLegend>
          <FieldGroup className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FormNumber
              name="distanceKm"
              label="Distance (km)"
              step="0.1"
              control={form.control}
            />
            <div className="grid grid-cols-2 gap-3">
              <FormNumber
                name="durationHours"
                label="Hours"
                step="1"
                control={form.control}
              />
              <FormNumber
                name="durationMinutes"
                label="Minutes"
                step="1"
                control={form.control}
              />
            </div>
            <FormDateTime
              name="startedAt"
              label="Started at"
              control={form.control}
            />
            <FormDateTime
              name="completedAt"
              label="Completed at"
              control={form.control}
            />
            <FormText name="truckMake" label="Truck make" control={form.control} />
            <FormText name="truckModel" label="Truck model" control={form.control} />
            <Controller
              name="status"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field data-invalid={fieldState.invalid}>
                  <FieldLabel htmlFor="job-status">Status</FieldLabel>
                  <Select value={field.value} onValueChange={field.onChange}>
                    <SelectTrigger id="job-status" aria-invalid={fieldState.invalid}>
                      <SelectValue placeholder="Select status" />
                    </SelectTrigger>
                    <SelectContent>
                      {STATUS_OPTIONS.map((o) => (
                        <SelectItem key={o.value} value={o.value}>
                          {o.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  {fieldState.invalid && fieldState.error && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
          </FieldGroup>
        </FieldSet>

        <FieldSeparator />

        {/* ---- Optional ---- */}
        <FieldSet>
          <FieldLegend>Optional</FieldLegend>
          <FieldGroup className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <FormText
              name="fuelUsedL"
              label="Fuel used (L)"
              description="Leave blank if unknown."
              control={form.control}
              inputMode="decimal"
            />
            <FormText
              name="damageDelta"
              label="Damage delta"
              description="0 (no damage) to 1 (totalled). Optional."
              control={form.control}
              inputMode="decimal"
            />
            <Controller
              name="notes"
              control={form.control}
              render={({ field, fieldState }) => (
                <Field
                  className="sm:col-span-2"
                  data-invalid={fieldState.invalid}
                >
                  <FieldLabel htmlFor="job-notes">Notes</FieldLabel>
                  <Textarea
                    {...field}
                    id="job-notes"
                    rows={3}
                    aria-invalid={fieldState.invalid}
                  />
                  {fieldState.invalid && fieldState.error && (
                    <FieldError errors={[fieldState.error]} />
                  )}
                </Field>
              )}
            />
          </FieldGroup>
        </FieldSet>

        <div className="flex justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={() => navigate("/jobs")}
          >
            Cancel
          </Button>
          <Button type="submit" disabled={form.formState.isSubmitting}>
            {form.formState.isSubmitting ? "Saving..." : "Save job"}
          </Button>
        </div>
      </form>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Small field shortcuts
// ---------------------------------------------------------------------------

type FormControl = Control<JobFormValues>;

interface BaseFieldProps {
  name: keyof JobFormValues;
  label: string;
  description?: string;
  control: FormControl;
}

function FormText({
  name,
  label,
  description,
  control,
  inputMode,
}: BaseFieldProps & { inputMode?: React.HTMLAttributes<HTMLInputElement>["inputMode"] }) {
  const id = `job-${name}`;
  return (
    <Controller
      name={name}
      control={control}
      render={({ field, fieldState }) => (
        <Field data-invalid={fieldState.invalid}>
          <FieldLabel htmlFor={id}>{label}</FieldLabel>
          <Input
            {...field}
            id={id}
            value={(field.value ?? "") as string}
            inputMode={inputMode}
            aria-invalid={fieldState.invalid}
          />
          {description && <FieldDescription>{description}</FieldDescription>}
          {fieldState.invalid && fieldState.error && (
            <FieldError errors={[fieldState.error]} />
          )}
        </Field>
      )}
    />
  );
}

function FormNumber({
  name,
  label,
  description,
  control,
  step,
}: BaseFieldProps & { step?: string }) {
  const id = `job-${name}`;
  return (
    <Controller
      name={name}
      control={control}
      render={({ field, fieldState }) => (
        <Field data-invalid={fieldState.invalid}>
          <FieldLabel htmlFor={id}>{label}</FieldLabel>
          <Input
            {...field}
            id={id}
            type="number"
            step={step}
            value={(field.value ?? "") as string}
            aria-invalid={fieldState.invalid}
          />
          {description && <FieldDescription>{description}</FieldDescription>}
          {fieldState.invalid && fieldState.error && (
            <FieldError errors={[fieldState.error]} />
          )}
        </Field>
      )}
    />
  );
}

function FormDateTime({
  name,
  label,
  description,
  control,
}: BaseFieldProps) {
  const id = `job-${name}`;
  return (
    <Controller
      name={name}
      control={control}
      render={({ field, fieldState }) => (
        <Field data-invalid={fieldState.invalid}>
          <FieldLabel htmlFor={id}>{label}</FieldLabel>
          <Input
            {...field}
            id={id}
            type="datetime-local"
            value={(field.value ?? "") as string}
            aria-invalid={fieldState.invalid}
          />
          {description && <FieldDescription>{description}</FieldDescription>}
          {fieldState.invalid && fieldState.error && (
            <FieldError errors={[fieldState.error]} />
          )}
        </Field>
      )}
    />
  );
}
