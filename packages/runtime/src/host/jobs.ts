import type { MonsterQuery } from "@lucent/game";
import type { CombatProfileDefinition } from "@lucent/schemas/combatProfiles";
import * as Effect from "effect/Effect";
import type * as Scope from "effect/Scope";

export interface ProfileJob {
  readonly id: string;
  readonly label: string;
  readonly classNames: readonly string[];
  readonly warnings: readonly string[];
  readonly failed: Effect.Effect<never>;
}
type Preparation = Pick<ProfileJob, "label" | "warnings" | "classNames">;
interface PendingJob {
  preparation?: Preparation | undefined;
  prepared?: ((answer: Preparation) => void) | undefined;
  error?: Error;
  listeners: Set<(error: Error) => void>;
}
export const profileJobHost = globalThis.__host;
const jobs = new Map<string, PendingJob>();

export const startProfileJob = (
  request: {
    readonly profile: CombatProfileDefinition;
    readonly targetPriority: readonly MonsterQuery[];
  },
  host: Pick<typeof __host, "startJob" | "cancelJob">,
): Effect.Effect<ProfileJob, never, Scope.Scope> => Effect.gen(function* () {
  const id = yield* Effect.acquireRelease(
    Effect.sync(() => {
      const id = host.startJob(JSON.stringify(request));
      jobs.set(id, { listeners: new Set() });
      return id;
    }),
    (id) => Effect.sync(() => {
      host.cancelJob(id);
      jobs.delete(id);
    }),
  );
  const failed: Effect.Effect<never> = Effect.callback((resume) => {
    const pending = jobs.get(id);
    if (pending?.error !== undefined) {
      resume(Effect.die(pending.error));
      return;
    }
    const fail = (error: Error) => resume(Effect.die(error));
    pending?.listeners.add(fail);
    return Effect.sync(() => { pending?.listeners.delete(fail); });
  });
  return yield* Effect.callback<ProfileJob>((resume) => {
    const pending = jobs.get(id);
    if (pending === undefined) {
      resume(Effect.die(new Error("Combat profile job was cancelled")));
      return;
    }
    pending.prepared = (answer) => {
      pending.preparation = undefined;
      resume(Effect.succeed({ id, ...answer, failed }));
    };
    const fail = (error: Error) => resume(Effect.die(error));
    pending.listeners.add(fail);
    if (pending.error !== undefined) fail(pending.error);
    else if (pending.preparation !== undefined) {
      pending.prepared(pending.preparation);
    }
    return Effect.sync(() => {
      pending.prepared = undefined;
      pending.listeners.delete(fail);
    });
  });
});

export const prepareJob = (
  id: string,
  label: string,
  warnings: readonly string[],
  classNames: readonly string[],
): void => {
  const pending = jobs.get(id);
  if (pending === undefined || pending.error !== undefined) return;
  const answer = { label, warnings, classNames };
  pending.preparation = answer;
  pending.prepared?.(answer);
};
export const failJob = (id: string, error: string): void => {
  const pending = jobs.get(id);
  if (pending === undefined) return;
  pending.preparation = undefined;
  pending.error = new Error(error);
  for (const fail of pending.listeners) fail(pending.error);
};
export const failJobs = (error: Error): void => {
  for (const id of jobs.keys()) failJob(id, error.message);
};
