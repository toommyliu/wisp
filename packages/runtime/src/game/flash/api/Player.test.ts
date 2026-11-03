import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";

import { makeFakeApi, makeFakeHostState } from "../testing/FakeHost";

describe("Player.isReady", () => {
  it.effect("reads the host readiness record without loader calls", () =>
    Effect.gen(function* () {
      const state = makeFakeHostState();
      state.missing = ["inventory", "player"];
      const target = { swf: new Proxy({}, { get() { throw new Error("unexpected loader call"); } }) } as Window;
      const api = yield* makeFakeApi(target, state);

      expect(yield* api.player.isReady()).toBe(false);
      state.missing = [];
      expect(yield* api.player.isReady()).toBe(false);
      state.canAct = true;
      expect(yield* api.player.isReady()).toBe(true);
      state.canAct = false;
      expect(yield* api.player.isReady()).toBe(false);
    }),
  );
});
