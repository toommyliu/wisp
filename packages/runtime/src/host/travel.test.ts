import { expect, it, vi } from "vitest";
import * as Effect from "effect/Effect";
import { completeTravel, startTravel } from "./travel";

it("returns a native travel answer and releases its job", async () => {
  const host = {
    startTravel: vi.fn((_request: string) => "travel-1"),
    cancelTravel: vi.fn(),
  };
  const result = Effect.runPromise(startTravel({ kind: "jump", cell: "r2", pad: "Left" }, host));
  await vi.waitFor(() => expect(host.startTravel).toHaveBeenCalledOnce());
  completeTravel("travel-1", true);
  expect(await result).toBe(true);
  expect(JSON.parse(host.startTravel.mock.calls[0]![0]!)).toEqual({ kind: "jump", cell: "r2", pad: "Left" });
  expect(host.cancelTravel).toHaveBeenCalledWith("travel-1");
});

it("cancels an interrupted travel without ending its caller", async () => {
  const host = { startTravel: vi.fn(() => "travel-2"), cancelTravel: vi.fn() };
  const result = Effect.runPromise(Effect.raceFirst(
    startTravel({ kind: "exit" }, host),
    Effect.yieldNow.pipe(Effect.as(false)),
  ));
  expect(await result).toBe(false);
  expect(host.cancelTravel).toHaveBeenCalledWith("travel-2");
  completeTravel("travel-2", true);
});

it("returns false when the native input limit rejects a request", async () => {
  const host = { startTravel: vi.fn(() => undefined), cancelTravel: vi.fn() };
  expect(await Effect.runPromise(startTravel({ kind: "jump", cell: "x".repeat(4097) }, host))).toBe(false);
  expect(host.cancelTravel).not.toHaveBeenCalled();
});
