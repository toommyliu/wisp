import { expect, it, vi } from "vitest";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Scheduler from "effect/Scheduler";

import { prepareJob, startProfileJob } from "./jobs";

it.each([8, 12, 16, 20, 24, 28, 32])("retains early preparation with an operation budget of %i", async (budget) => {
  const id = `early-${budget}`;
  const host = {
    startJob: vi.fn(() => {
      queueMicrotask(() => prepareJob(id, "Profile", [], ["Bard"]));
      return id;
    }),
    cancelJob: vi.fn(),
  };
  const result = await Effect.runPromise(
    Effect.scoped(
      startProfileJob({ profile: { steps: [{ skill: 1 }] }, targetPriority: [] }, host),
    ).pipe(
      Effect.provideService(Scheduler.MaxOpsBeforeYield, budget),
      Effect.timeoutOption("1 second"),
    ),
  );
  expect(host.startJob).toHaveBeenCalledOnce();
  expect(Option.isSome(result)).toBe(true);
  if (Option.isSome(result)) {
    expect(result.value.id).toBe(id);
    expect(result.value.classNames).toEqual(["Bard"]);
  }
  expect(host.cancelJob).toHaveBeenCalledExactlyOnceWith(id);
});
