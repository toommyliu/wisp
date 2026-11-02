import { afterEach, expect, it, vi } from "vitest";
import * as Effect from "effect/Effect";

import { makeFakeApi, makeFakeHostState } from "../game/flash/testing/FakeHost";
import { operate, completeOperation, startOperation, setOperationResponsibility } from "./operations";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("answers false when the host loses an accept answer", async () => {
  vi.useFakeTimers();
  const prepare = vi.fn(() => "lost");
  const host = { prepareOperation: prepare, operate: vi.fn(), cancelOperation: vi.fn() };
  vi.stubGlobal("__host", host);
  let result: boolean | undefined;
  const finished = Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    const api = yield* makeFakeApi({ swf: {} } as Window, makeFakeHostState());
    result = yield* api.drops.accept(70);
  })));
  await vi.waitFor(() => expect(prepare).toHaveBeenCalledOnce());
  startOperation("lost");
  await vi.advanceTimersByTimeAsync(15_000);
  expect(result).toBeUndefined();
  await vi.advanceTimersByTimeAsync(20_000);
  expect(result).toBe(false);
  await finished;
  expect(host.cancelOperation).toHaveBeenCalledExactlyOnceWith("lost");
  completeOperation("lost", { kind: "acceptDrop", value: true }, null);
  expect(result).toBe(false);
});

it("keeps a slow real accept alive and clears its deadline after the answer", async () => {
  vi.useFakeTimers();
  const prepare = vi.fn(() => "slow");
  vi.stubGlobal("__host", { prepareOperation: prepare, operate: vi.fn(), cancelOperation: vi.fn() });
  let result: boolean | undefined;
  const finished = Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    const api = yield* makeFakeApi({ swf: {} } as Window, makeFakeHostState());
    result = yield* api.drops.accept(70);
  })));
  await vi.waitFor(() => expect(prepare).toHaveBeenCalledOnce());
  startOperation("slow");
  await vi.advanceTimersByTimeAsync(15_000);
  expect(result).toBeUndefined();
  completeOperation("slow", { kind: "acceptDrop", value: true }, null);
  await finished;
  expect(result).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
});

it("keeps queued accepts waiting beyond the execution deadline", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("__host", { prepareOperation: () => "queued", operate: vi.fn(), cancelOperation: vi.fn() });
  let result: boolean | undefined;
  const finished = Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    const api = yield* makeFakeApi({ swf: {} } as Window, makeFakeHostState());
    result = yield* api.drops.accept(70);
  })));
  await vi.advanceTimersByTimeAsync(36_000);
  expect(result).toBeUndefined();
  completeOperation("queued", { kind: "acceptDrop", value: true }, null);
  await finished;
  expect(result).toBe(true);
});

it("cancels an interrupted accept and ignores its late answer", async () => {
  const cancelOperation = vi.fn();
  const prepare = vi.fn(() => "interrupted");
  vi.stubGlobal("__host", { prepareOperation: prepare, operate: vi.fn(), cancelOperation });
  let resumed = false;
  const result = await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    const api = yield* makeFakeApi({ swf: {} } as Window, makeFakeHostState());
    return yield* api.drops.accept(70).pipe(
      Effect.tap(() => Effect.sync(() => { resumed = true; })),
      Effect.timeoutOption("1 millis"),
    );
  })));
  expect(result._tag).toBe("None");
  expect(cancelOperation).toHaveBeenCalledExactlyOnceWith("interrupted");
  completeOperation("interrupted", { kind: "acceptDrop", value: true }, null);
  expect(resumed).toBe(false);
});

it("fails queued and running operations when their event stream overflows", async () => {
  const host = { now: () => 0, prepareOperation: () => "queued-lost", operate: vi.fn(), cancelOperation: vi.fn() };
  vi.stubGlobal("__host", host);
  vi.stubGlobal("window", {});
  const { turn } = await import("./loop");
  let error: unknown;
  const operating = Effect.runPromise(operate({ kind: "acceptDrop", selector: 1 }, host)).catch(cause => { error = cause; });
  turn('[{"type":"eventslost"}]');
  await Promise.resolve();
  await operating;
  expect(host.cancelOperation).toHaveBeenCalledWith("queued-lost");
  expect(error).toBeInstanceOf(Error);
  expect((error as Error).message).toBe("Host events were lost; pending requests failed");
});

it("registers completion before submission and starts each evaluation lazily", async () => {
  const host = {
    prepareOperation: vi.fn().mockReturnValueOnce("one").mockReturnValueOnce("two"),
    operate: vi.fn((id: string) => completeOperation(id, { kind: "acceptDrop", value: true }, null)),
    cancelOperation: vi.fn(),
  };
  const operation = operate({ kind: "acceptDrop", selector: 70 }, host);
  expect(host.prepareOperation).not.toHaveBeenCalled();
  expect(await Effect.runPromise(operation)).toEqual({ kind: "acceptDrop", value: true });
  expect(await Effect.runPromise(operation)).toEqual({ kind: "acceptDrop", value: true });
  expect(host.operate.mock.calls.map(([id]) => id)).toEqual(["one", "two"]);
  expect(host.cancelOperation).not.toHaveBeenCalled();
});

it("cancels a reserved handle when submission fails", async () => {
  const host = {
    prepareOperation: () => "failed",
    operate: () => { throw new Error("submission failed"); },
    cancelOperation: vi.fn(),
  };
  await expect(Effect.runPromise(operate({ kind: "acceptDrop", selector: 70 }, host))).rejects.toThrow("submission failed");
  expect(host.cancelOperation).toHaveBeenCalledExactlyOnceWith("failed");
  completeOperation("failed", { kind: "acceptDrop", value: true }, null);
});


it("bounds a shared equip only after it takes responsibility", async () => {
  vi.useFakeTimers();
  const host = { prepareOperation: () => "shared", operate: vi.fn(), cancelOperation: vi.fn() };
  let error: unknown;
  const finished = Effect.runPromise(operate({ kind: "prepareConsumableClaim", claim: "1" }, host))
    .catch(cause => { error = cause; });
  setOperationResponsibility("shared", false);
  await vi.advanceTimersByTimeAsync(6_000);
  expect(error).toBeUndefined();
  expect(host.cancelOperation).not.toHaveBeenCalled();
  setOperationResponsibility("shared", true);
  await vi.advanceTimersByTimeAsync(4_999);
  expect(error).toBeUndefined();
  await vi.advanceTimersByTimeAsync(1);
  await finished;
  expect(error).toBeInstanceOf(Error);
  expect(host.cancelOperation).toHaveBeenCalledExactlyOnceWith("shared");
});

it("settles a successful shared equip without issuing another equip", async () => {
  vi.useFakeTimers();
  const host = { prepareOperation: () => "ready", operate: vi.fn(), cancelOperation: vi.fn() };
  const finished = Effect.runPromise(operate({ kind: "prepareConsumableClaim", claim: "1" }, host));
  setOperationResponsibility("ready", false);
  await vi.advanceTimersByTimeAsync(6_000);
  setOperationResponsibility("ready", true);
  completeOperation("ready", { kind: "equipConsumable", value: true }, null);
  expect(await finished).toEqual({ kind: "equipConsumable", value: true });
  expect(host.cancelOperation).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it.each([
  {
    operation: { kind: "loadQuests", ids: Array.from({ length: 91 }, (_, index) => index + 1), silent: true },
    answer: { kind: "loadQuests", value: Array.from({ length: 91 }, () => false) },
  },
  {
    operation: { kind: "openBank", force: true },
    answer: { kind: "openBank", value: true },
  },
] satisfies readonly {
  operation: import("@lucent/game/protocol").Operation;
  answer: import("@lucent/game/protocol").OperationAnswer;
}[])("keeps a bounded $operation.kind transaction alive beyond 35 seconds", async ({ operation, answer }) => {
  vi.useFakeTimers();
  const host = { prepareOperation: () => "long-wait", operate: vi.fn(), cancelOperation: vi.fn() };
  let error: unknown;
  const finished = Effect.runPromise(operate(operation, host)).catch(cause => { error = cause; });
  startOperation("long-wait");
  await vi.advanceTimersByTimeAsync(40_000);
  expect(error).toBeUndefined();
  expect(host.cancelOperation).not.toHaveBeenCalled();
  completeOperation("long-wait", answer, null);
  expect(await finished).toEqual(answer);
  expect(vi.getTimerCount()).toBe(0);
});
