import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  makeScriptQueue,
  type ScriptQueueState,
} from "../../../../../packages/runtime/src/game/scripting/ScriptQueue.ts";
import type { ScriptRunTerminalOutcome } from "../../../../../packages/runtime/src/game/scripting/ScriptRunner.ts";

type ScriptFile = Parameters<
  Parameters<typeof makeScriptQueue>[0]["startScript"]
>[0];

type Operation = {
  op: string;
  name?: string;
  available?: boolean;
  offset?: -1 | 1;
  kind?: ScriptRunTerminalOutcome["kind"];
  failure?: boolean;
  hold_start?: boolean;
  expect: {
    entries: string[];
    available: boolean[];
    phase: string;
    status: string | null;
    current: number | null;
    items: string[];
    starts: string[];
  };
};
const corpus: { name: string; operations: Operation[] }[] = JSON.parse(
  await readFile(new URL("./queue-corpus.json", import.meta.url), "utf8"),
);
function file(name: string, available = false): ScriptFile {
  return {
    name,
    path: `${name}.js`,
    revision: "test",
    source: "export default function*() {}",
    inputs: available
      ? {
          id: name,
          fields: [{ key: "target", label: "Target", type: "string" }],
        }
      : null,
  };
}
function outcome(
  kind: ScriptRunTerminalOutcome["kind"],
): ScriptRunTerminalOutcome {
  const date = "2026-10-31T00:00:00.000Z";
  switch (kind) {
    case "completed":
      return {
        kind,
        status: { state: "completed", name: "test", completedAt: date },
      };
    case "failed":
      return {
        kind,
        status: {
          state: "failed",
          name: "test",
          message: "boom",
          failedAt: date,
        },
      };
    case "script-exited":
      return {
        kind,
        closeClient: false,
        status: { state: "stopped", stoppedAt: date },
      };
    case "script-stopped":
    case "externally-stopped":
      return { kind, status: { state: "stopped", stoppedAt: date } };
  }
}
function waitFor(
  queue: ReturnType<typeof makeScriptQueue>,
  predicate: (state: ScriptQueueState) => boolean,
) {
  if (predicate(queue.getState())) return Promise.resolve();
  return new Promise<void>((resolve) => {
    const dispose = queue.onState((state) => {
      if (!predicate(state)) return;
      dispose();
      resolve();
    });
  });
}
for (const scenario of corpus) {
  let serial = 0;
  const ids = new Map<string, string>();
  const starts: string[] = [];
  const resolutions: {
    name: string;
    resolve: (file: ScriptFile) => void;
    reject: (error: Error) => void;
  }[] = [];
  let editing: ScriptFile | undefined;
  let terminal = Promise.withResolvers<ScriptRunTerminalOutcome>();
  let holdStart = false;
  let startGate = Promise.withResolvers<{
    terminal: Promise<ScriptRunTerminalOutcome>;
  }>();
  let starting: Promise<boolean> | undefined;
  let canceling: Promise<void> | undefined;
  const queue = makeScriptQueue({
    createId: () => String(++serial),
    now: () => 0,
    confirmStandaloneReplacement: async () => true,
    isRunnerActive: async () => false,
    onUnexpectedError: () => {},
    requestInputs: async ({ values }) => values,
    resolve: async (ref) =>
      editing ??
      (await new Promise<ScriptFile>((resolve, reject) =>
        resolutions.push({ name: ref.name, resolve, reject }),
      )),
    startScript: async (script) => {
      starts.push(script.name);
      terminal = Promise.withResolvers();
      return holdStart
        ? await startGate.promise
        : { terminal: terminal.promise };
    },
    stopScript: async () => {},
  });
  for (const op of scenario.operations) {
    switch (op.op) {
      case "add": {
        const id = await queue.add(file(op.name!, op.available), {});
        if (id !== null) ids.set(op.name!, id);
        break;
      }
      case "edit":
        editing = file(op.name!, op.available);
        await queue.editInputs(ids.get(op.name!)!);
        editing = undefined;
        break;
      case "remove":
        queue.remove(ids.get(op.name!)!);
        break;
      case "move":
        queue.move(ids.get(op.name!)!, op.offset!);
        break;
      case "clear":
        queue.clear();
        break;
      case "start":
        starting = queue.start();
        break;
      case "prepared": {
        holdStart = op.hold_start ?? false;
        const waiting = resolutions.splice(0);
        for (const resolution of waiting) {
          if (op.failure) resolution.reject(new Error("resolution failed"));
          else resolution.resolve(file(resolution.name, op.available));
        }
        if (starting !== undefined) await starting;
        break;
      }
      case "cancel":
        canceling = queue.cancel();
        if (queue.getState().phase !== "stopping") await canceling;
        break;
      case "next":
        queue.runNext();
        break;
      case "terminal": {
        startGate.resolve({ terminal: terminal.promise });
        terminal.resolve(outcome(op.kind!));
        await waitFor(
          queue,
          (s) =>
            s.phase === op.expect.phase && s.currentIndex === op.expect.current,
        );
        if (canceling) await canceling;
        break;
      }
      default:
        throw new Error(`Unknown operation ${op.op}`);
    }
    const s = queue.getState();
    assert.deepEqual(
      {
        entries: s.entries.map((e) => e.file.name),
        available: s.entries.map((e) => e.inputsAvailable),
        phase: s.phase,
        status: s.latestRun?.status ?? null,
        current: s.currentIndex,
        items: s.latestRun?.items.map((i) => i.result?.kind ?? i.state) ?? [],
        starts: [...starts],
      },
      op.expect,
      `${scenario.name}: ${op.op}`,
    );
  }
  queue.dispose();
  console.log(`PASS ${scenario.name}`);
}
console.log(`TS ScriptQueue: ${corpus.length} shared scenarios passed`);
