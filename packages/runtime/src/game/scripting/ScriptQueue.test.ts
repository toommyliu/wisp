import { describe, expect, it, vi } from "@effect/vitest";

import type { ScriptFile } from "@lucent/schemas/scriptInputs";
import {
  makeScriptQueue,
  type ScriptQueueDependencies,
  type ScriptQueueSession,
  type ScriptQueueState,
} from "./ScriptQueue";
import type { ScriptRunTerminalOutcome } from "./ScriptRunner";

const file = (name: string): ScriptFile => ({
  inputs: null,
  name,
  path: `/scripts/${name}.js`,
  revision: `${name}-revision`,
  source: "export function* main() {}",
});

const completed = (script: ScriptFile): ScriptRunTerminalOutcome => ({
  kind: "completed",
  status: {
    completedAt: "2026-08-18T12:00:00.000Z",
    name: script.name,
    state: "completed",
  },
});

const failed = (script: ScriptFile): ScriptRunTerminalOutcome => ({
  kind: "failed",
  status: {
    failedAt: "2026-08-18T12:00:00.000Z",
    message: "boom",
    name: script.name,
    state: "failed",
  },
});

const externallyStopped = (): ScriptRunTerminalOutcome => ({
  kind: "externally-stopped",
  status: {
    reason: "Queue canceled",
    state: "stopped",
    stoppedAt: "2026-08-18T12:00:00.000Z",
  },
});

const dependencies = (
  overrides: Partial<ScriptQueueDependencies> = {},
): ScriptQueueDependencies => {
  let id = 0;
  return {
    confirmStandaloneReplacement: () => Promise.resolve(true),
    createId: (prefix) => `${prefix}-${++id}`,
    isRunnerActive: () => Promise.resolve(false),
    onUnexpectedError: vi.fn(),
    requestInputs: ({ values }) => Promise.resolve(values),
    resolve: (reference) => Promise.resolve(file(reference.name)),
    startScript: (script) =>
      Promise.resolve({ terminal: Promise.resolve(completed(script)) }),
    stopScript: () => Promise.resolve(),
    ...overrides,
  };
};

const waitForState = (
  queue: ReturnType<typeof makeScriptQueue>,
  predicate: (state: ScriptQueueState) => boolean,
): Promise<ScriptQueueState> => {
  const current = queue.getState();
  if (predicate(current)) return Promise.resolve(current);

  return new Promise((resolve) => {
    const dispose = queue.onState((state) => {
      if (!predicate(state)) return;
      dispose();
      resolve(state);
    });
  });
};

const waitForPhase = (
  queue: ReturnType<typeof makeScriptQueue>,
  phase: ScriptQueueState["phase"],
): Promise<ScriptQueueState> =>
  waitForState(queue, (state) => state.phase === phase);

const queuedPair = async () => {
  const first = file("first");
  const second = file("second");
  const firstTerminal = Promise.withResolvers<ScriptRunTerminalOutcome>();
  const secondTerminal = Promise.withResolvers<ScriptRunTerminalOutcome>();
  const startScript = vi
    .fn<(script: ScriptFile) => Promise<ScriptQueueSession>>()
    .mockResolvedValueOnce({ terminal: firstTerminal.promise })
    .mockResolvedValueOnce({ terminal: secondTerminal.promise });
  const queue = makeScriptQueue(dependencies({ startScript }));
  await queue.add(first, {});
  await queue.add(second, {});
  await queue.start();
  return { first, firstTerminal, queue, second, secondTerminal, startScript };
};

describe("ScriptQueue", () => {
  it("refreshes input availability when a queued script is resolved again", async () => {
    const script: ScriptFile = {
      ...file("configurable"),
      inputs: {
        id: "configurable",
        fields: [{ key: "target", label: "Target", type: "string" }],
      },
    };
    const resolve = vi
      .fn<ScriptQueueDependencies["resolve"]>()
      .mockResolvedValueOnce({ ...script, inputs: null })
      .mockResolvedValueOnce(script);
    const queue = makeScriptQueue(dependencies({ resolve }));
    const entryId = await queue.add(script, {});
    if (entryId === null) throw new Error("Script was not added");
    expect(queue.getState().entries[0]?.inputsAvailable).toBe(true);

    await queue.editInputs(entryId);
    expect(queue.getState().entries[0]?.inputsAvailable).toBe(false);

    await queue.start();
    const finished = await waitForPhase(queue, "idle");
    expect(finished.entries[0]?.inputsAvailable).toBe(true);
  });

  it("runs entries in order and records their terminal outcomes", async () => {
    const { first, firstTerminal, queue, second, secondTerminal, startScript } =
      await queuedPair();
    expect(startScript).toHaveBeenCalledTimes(1);

    firstTerminal.resolve(completed(first));
    await waitForState(queue, (state) => state.currentIndex === 1);
    expect(startScript).toHaveBeenCalledTimes(2);
    expect(startScript.mock.calls.map(([script]) => script)).toEqual([
      first,
      second,
    ]);

    secondTerminal.resolve(completed(second));
    const finished = await waitForPhase(queue, "idle");
    expect(finished.latestRun?.status).toBe("completed");
    expect(finished.latestRun?.items.map((item) => item.result?.kind)).toEqual([
      "completed",
      "completed",
    ]);
  });

  it("pauses after a failure and resumes with the next entry", async () => {
    const { first, firstTerminal, queue, second, secondTerminal, startScript } =
      await queuedPair();
    firstTerminal.resolve(failed(first));

    const paused = await waitForPhase(queue, "paused");
    expect(paused.currentIndex).toBe(0);
    expect(startScript).toHaveBeenCalledTimes(1);

    queue.runNext();
    expect(startScript).toHaveBeenCalledTimes(2);
    secondTerminal.resolve(completed(second));
    const finished = await waitForPhase(queue, "idle");
    expect(finished.latestRun?.items[0]?.result?.kind).toBe("failed");
    expect(finished.latestRun?.items[1]?.result?.kind).toBe("completed");
  });

  it.each([
    ["failed", "failed"],
    ["externally-stopped", "stopped"],
    ["script-exited", "exited"],
    ["script-stopped", "completed"],
  ] as const)(
    "finishes a final %s outcome as %s",
    async (kind, expectedStatus) => {
      const script = file("terminal");
      const outcome: ScriptRunTerminalOutcome =
        kind === "failed"
          ? failed(script)
          : kind === "externally-stopped"
            ? externallyStopped()
            : {
                kind,
                closeClient: false,
                logout: false,
                status: {
                  state: "stopped",
                  stoppedAt: "2026-08-18T12:00:00.000Z",
                },
              };
      const queue = makeScriptQueue(
        dependencies({
          startScript: () =>
            Promise.resolve({ terminal: Promise.resolve(outcome) }),
        }),
      );

      await queue.add(script, {});
      await queue.start();
      const finished = await waitForPhase(queue, "idle");

      expect(finished.latestRun?.status).toBe(expectedStatus);
    },
  );

  it("ignores preparation that resolves after cancellation", async () => {
    const script = file("slow");
    const resolution = Promise.withResolvers<ScriptFile>();
    const startScript = vi.fn<ScriptQueueDependencies["startScript"]>();
    const queue = makeScriptQueue(
      dependencies({
        resolve: () => resolution.promise,
        startScript,
      }),
    );

    await queue.add(script, {});
    const starting = queue.start();
    expect(queue.getState().phase).toBe("preparing");

    await queue.cancel();
    resolution.resolve(script);
    await expect(starting).resolves.toBe(false);
    expect(startScript).not.toHaveBeenCalled();
    expect(queue.getState().phase).toBe("idle");
  });

  it("can stop while startScript is still pending", async () => {
    const script = file("slow-start");
    const session = Promise.withResolvers<ScriptQueueSession>();
    const calls: string[] = [];
    const queue = makeScriptQueue(
      dependencies({
        startScript: async () => {
          calls.push("start");
          return await session.promise;
        },
        stopScript: async () => {
          calls.push("stop");
          session.resolve({ terminal: Promise.resolve(externallyStopped()) });
        },
      }),
    );

    await queue.add(script, {});
    await queue.start();
    await queue.cancel();

    expect(calls).toEqual(["start", "stop"]);
    expect(queue.getState().phase).toBe("idle");
    expect(queue.getState().latestRun?.status).toBe("canceled");
  });

  it("stops the active runner after replacement is confirmed", async () => {
    const stopScript = vi.fn<ScriptQueueDependencies["stopScript"]>();
    const queue = makeScriptQueue(
      dependencies({
        isRunnerActive: () => Promise.resolve(true),
        stopScript,
      }),
    );

    await queue.add(file("replacement"), {});
    await queue.start();

    expect(stopScript).toHaveBeenCalledOnce();
    expect(stopScript).toHaveBeenCalledWith("Replaced by script queue");
  });
});
