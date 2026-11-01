import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import * as Effect from "effect/Effect";
import * as Random from "effect/Random";
import * as Deferred from "effect/Deferred";
import * as Fiber from "effect/Fiber";
import { EntityState } from "@lucent/game";
import type { RoomPolicy } from "@lucent/schemas/accountSettings";
import {
  makeFakeApi,
  makeFakeHostState,
  player,
  item,
  monster,
} from "../flash/testing/FakeHost";
import { makeBridge } from "../flash/bridge/Bridge";
import { DiagnosticSink } from "../flash/bridge/DiagnosticSink";
import {
  makeMoveToSafeDestination,
  runWithSafeStartStop,
} from "./safeStartStop";

type Snapshot = {
  logged_in: boolean;
  username: string;
  equipped_house: boolean;
  own_house: boolean;
  travel: {
    map: string;
    room: number;
    loaded: boolean;
    ready: boolean;
    tfer: boolean;
    alive: boolean;
    state: "idle" | "combat" | "dead";
    cell: string;
    pad: string;
    live_cell: string;
    live_pad: string;
    cells: string[];
    monsters: { cell: string; aggressive: boolean }[];
  };
};
type Patch = Partial<Omit<Snapshot, "travel">> & {
  travel?: Partial<Snapshot["travel"]>;
};
type Scenario = {
  name: string;
  initial: Snapshot;
  policy:
    { kind: "public" } | { kind: "specific" | "random-private"; room: number };
  commands: {
    method: string;
    args: unknown[];
    at: number;
    reply: unknown;
    patch: Patch;
  }[];
  updates: { at: number; patch: Patch }[];
  finished_at: number;
  lifecycle?: boolean;
};
const scenarios = JSON.parse(
  readFileSync(
    new URL(
      "../../../../../crates/host/tests/fixtures/safe_move/transcripts.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as Scenario[];

describe("shared safe move transcripts", () => {
  for (const scenario of scenarios)
    it(scenario.name, async () => {
      vi.useFakeTimers();
      try {
        const started = Date.now();
        const observed = structuredClone(scenario.initial);
        const state = makeFakeHostState();
        let index = 0;
        const calls: { method: string; args: unknown[]; at: number }[] = [];
        const sync = () => {
          const t = observed.travel;
          state.loggedIn = observed.logged_in;
          state.username = observed.username;
          state.map = { id: 1, name: t.map, roomNumber: t.room };
          state.missing = t.ready ? [] : ["player"];
          state.selfEntityId = 1;
          state.players = [
            player({
              entityId: 1,
              username: observed.username,
              cell: t.cell,
              pad: t.pad,
              hp: t.alive ? 100 : 0,
              state:
                t.state === "idle"
                  ? EntityState.Idle
                  : t.state === "dead"
                    ? EntityState.Dead
                    : EntityState.InCombat,
            }),
          ];
          state.cells = t.cells;
          state.monsters = t.monsters.map((m, i) =>
            monster({ ...m, monsterMapId: i + 1, name: "monster" }),
          );
          state.items.house = [
            item("house", {
              itemId: 1,
              name: "house item",
              category: observed.equipped_house ? "House" : "Floor Item",
              equipped: true,
            }),
          ];
        };
        const patch = (p: Patch) => {
          const { travel, ...rest } = p;
          Object.assign(observed, rest);
          Object.assign(observed.travel, travel);
          sync();
        };
        sync();
        const call = (method: string, args: unknown[]) => {
          const expected = scenario.commands[index++];
          calls.push({ method, args, at: Date.now() - started });
          if (!expected) throw new Error("unexpected loader call");
          patch(expected.patch);
          if (JSON.stringify(expected.reply) === '{"unavailable":true}')
            throw new Error("loader unavailable");
          if (method === "flash.isNull" || method === "world.isCellReady")
            return expected.reply;
          if (method === "flash.getGameObject")
            return JSON.stringify(expected.reply);
          if (expected.reply === false) throw new Error("loader unavailable");
          if (method === "flash.callGameFunction") return "";
          return undefined;
        };
        const target = {
          swf: {
            "auth.isLoggedIn": () => observed.logged_in,
            "flash.getGameObjectS": (path: string) =>
              path.includes("Username") ? observed.username : "",
            "world.isLoaded": () => observed.travel.loaded,
            "player.isLoaded": () => true,
            "world.isActionAvailable": () => observed.travel.tfer,
            "world.getCells": () => observed.travel.cells,
            "player.getCell": () => observed.travel.live_cell,
            "player.getPad": () => observed.travel.live_pad,
            "flash.callGameFunction0": () => JSON.stringify(observed.own_house),
            ...Object.fromEntries(
              [
                "flash.isNull",
                "flash.getGameObject",
                "flash.setGameObject",
                "flash.callGameFunction",
                "player.joinMap",
                "player.jump",
                "combat.cancelAutoAttack",
                "combat.cancelTarget",
                "world.isCellReady",
              ].map((method) => [
                method,
                (...args: unknown[]) => call(method, args),
              ]),
            ),
          },
        } as unknown as Window;
        for (const update of scenario.updates)
          setTimeout(() => patch(update.patch), update.at);
        const policy: RoomPolicy =
          scenario.policy.kind === "specific"
            ? { kind: "specific", roomNumber: scenario.policy.room }
            : { kind: scenario.policy.kind };
        let draws = 0;
        const run = Effect.gen(function* () {
          const api = yield* makeFakeApi(target, state);
          const bridge = yield* makeBridge(target);
          const move = makeMoveToSafeDestination({
            ...api,
            bridge,
            roomPolicy: Effect.succeed(policy),
          });
          if (scenario.lifecycle) {
            const started = yield* Deferred.make<void>();
            const phases: string[] = [];
            const fiber = yield* runWithSafeStartStop(
              Deferred.succeed(started, undefined).pipe(
                Effect.andThen(Effect.never),
              ),
              Effect.succeed(true),
              (phase) =>
                Effect.sync(() => {
                  phases.push(phase);
                }).pipe(Effect.andThen(move(phase))),
            ).pipe(Effect.forkScoped);
            yield* Deferred.await(started);
            yield* Fiber.interrupt(fiber);
            expect(phases).toEqual(["before", "after"]);
          } else yield* move("before");
        }).pipe(
          Effect.provideService(DiagnosticSink, { report: () => {} }),
          Effect.provideService(Random.Random, {
            nextDoubleUnsafe: () =>
              (("room" in scenario.policy ? scenario.policy.room : 10000) -
                10000 +
                draws++) /
              90000,
            nextIntUnsafe: () => 0,
          }),
        );
        let finishedAt = -1;
        const promise = Effect.runPromise(
          Effect.scoped(run).pipe(
            Effect.tap(() =>
              Effect.sync(() => {
                finishedAt = Date.now() - started;
              }),
            ),
          ),
        );
        await vi.runAllTimersAsync();
        await promise;
        expect(calls).toEqual(
          scenario.commands.map(({ method, args, at }) => ({
            method,
            args,
            at,
          })),
        );
        expect(finishedAt).toBe(scenario.finished_at);
      } finally {
        vi.useRealTimers();
      }
    });
});
