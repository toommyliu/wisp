import { afterEach, describe, expect, it, vi } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";

import type { BridgeService } from "../bridge/Bridge";
import type { HostStateService } from "../state/HostState";
import { makeQuests } from "./Quests";
import type { Wait } from "./Wait";

afterEach(() => vi.unstubAllGlobals());

const makeHarness = () => {
  vi.stubGlobal("__host", { acquireQuestClaim: () => JSON.stringify("1"), releaseClaim: () => {}, settleClaim: () => {} });
  const completions: Array<readonly unknown[]> = [];
  const bridge = {
    invoke: (method: string, args: readonly unknown[] | undefined) => {
      if (method === "quests.isInProgress") {
        return Effect.succeed(Option.some(true));
      }
      if (method === "quests.canComplete") {
        return Effect.succeed(Option.some(true));
      }
      if (method === "quests.getMaxTurnIns") {
        return Effect.succeed(Option.some(3));
      }
      if (method === "quests.complete") {
        completions.push(args ?? []);
        return Effect.succeed(Option.some(undefined));
      }
      return Effect.succeed(Option.none());
    },
  } as unknown as BridgeService;
  const wait = {
    forEvent: (
      _selector: unknown,
      options: { readonly trigger: Effect.Effect<boolean> },
    ) =>
      options.trigger.pipe(Effect.map((triggered) => (triggered ? {} : null))),
    forGameAction: () => Effect.succeed(true),
  } as unknown as Wait;

  return {
    completions,
    quests: makeQuests(bridge, {} as HostStateService, wait),
  };
};

describe("Quests.complete", () => {
  it.effect("translates completion options to Flash arguments", () =>
    Effect.gen(function* () {
      const { completions, quests } = makeHarness();

      expect(
        yield* quests.complete(123, { rewardItemId: 456, turnIns: 2 }),
      ).toBe(true);
      expect(completions).toEqual([[123, 2, 456, false]]);
    }),
  );

  it.effect("uses the maximum turn-ins when options are omitted", () =>
    Effect.gen(function* () {
      const { completions, quests } = makeHarness();

      expect(yield* quests.complete(123)).toBe(true);
      expect(completions).toEqual([[123, 3, -1, false]]);
    }),
  );

  it.effect("rejects non-finite turn-ins before invoking Flash", () =>
    Effect.gen(function* () {
      const { completions, quests } = makeHarness();

      expect(yield* quests.complete(123, { turnIns: Number.NaN })).toBe(false);
      expect(completions).toEqual([]);
    }),
  );
});
