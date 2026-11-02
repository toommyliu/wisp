import * as Effect from "effect/Effect";

export type TravelRequest =
  | { readonly kind: "join"; readonly target: string; readonly cell?: string | undefined; readonly pad?: string | undefined; readonly random_room: number }
  | { readonly kind: "jump"; readonly cell: string; readonly pad?: string | undefined }
  | { readonly kind: "exit" };
interface PendingTravel {
  answer?: boolean;
  resume?: ((value: boolean) => void) | undefined;
}
const pending = new Map<string, PendingTravel>();
export const travelHost = globalThis.__host;

export const startTravel = (
  request: TravelRequest,
  host: Pick<typeof __host, "startTravel" | "cancelTravel">,
): Effect.Effect<boolean> => Effect.scoped(Effect.gen(function* () {
  const id = yield* Effect.acquireRelease(
    Effect.sync(() => {
      const id = host.startTravel(JSON.stringify(request));
      if (id !== undefined) pending.set(id, {});
      return id;
    }),
    (id) => Effect.sync(() => {
      if (id !== undefined) {
        host.cancelTravel(id);
        pending.delete(id);
      }
    }),
  );
  if (id === undefined) return false;
  return yield* Effect.callback<boolean>((resume) => {
    const job = pending.get(id)!;
    job.resume = (value) => resume(Effect.succeed(value));
    if (job.answer !== undefined) job.resume(job.answer);
    return Effect.sync(() => { job.resume = undefined; });
  });
}));

export const completeTravel = (id: string, value: boolean): void => {
  const job = pending.get(id);
  if (job === undefined) return;
  job.answer = value;
  job.resume?.(value);
};
export const failTravels = (): void => {
  for (const id of pending.keys()) completeTravel(id, false);
};
