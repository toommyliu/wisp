import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import * as Effect from "effect/Effect";
import * as Random from "effect/Random";
import { EntityState } from "@lucent/game";
import {
  makeFakeApi,
  makeFakeHostState,
  monster,
  player,
} from "./FakeHost";
import { DiagnosticSink } from "../bridge/DiagnosticSink";

type Snapshot = {
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
type Scenario = {
  name: string;
  request: {
    kind: "join" | "jump" | "exit";
    target?: string;
    cell?: string;
    pad?: string;
    random_room?: number;
  };
  initial: Snapshot;
  commands: {
    method: string;
    at: number;
    args: string[];
    patch: Partial<Snapshot>;
    reply: boolean;
    delay_ms?: number;
  }[];
  updates: { at: number; patch: Partial<Snapshot> }[];
  outcome: boolean;
  finished_at: number;
};
const scenarios = JSON.parse(
  readFileSync(
    new URL(
      "../../../../../../crates/host/tests/fixtures/travel/transcripts.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as Scenario[];

describe("shared travel transcripts", () => {
  for (const scenario of scenarios) {
    it(scenario.name, async () => {
      vi.useFakeTimers();
      try {
        const started = Date.now();
        const observed = structuredClone(scenario.initial);
        const state = makeFakeHostState();
        let index = 0;
        const calls: { method: string; args: string[]; at: number }[] = [];
        const sync = () => {
          state.map = { id: 1, name: observed.map, roomNumber: observed.room };
          state.missing = observed.ready ? [] : ["player"];
          state.loggedIn = true;
          state.selfEntityId = 1;
          state.players = [
            player({
              entityId: 1,
              username: "test",
              cell: observed.cell,
              pad: observed.pad,
              hp: observed.alive ? 100 : 0,
              state:
                observed.state === "idle"
                  ? EntityState.Idle
                  : observed.state === "dead"
                    ? EntityState.Dead
                    : EntityState.InCombat,
            }),
          ];
          state.cells = observed.cells;
          state.monsters = observed.monsters.map((entry, i) =>
            monster({ ...entry, monsterMapId: i + 1, name: "monster" }),
          );
        };
        sync();
        const call = (method: string, args: string[]) => {
          const expected = scenario.commands[index++];
          calls.push({ method, args, at: Date.now() - started });
          if (!expected) throw new Error("unexpected loader call");
          Object.assign(observed, expected.patch);
          sync();
          if (method === "world.isCellReady") return expected.reply;
          if (!expected.reply) throw new Error("loader unavailable");
          return undefined;
        };
        const target = {
          swf: {
            "world.isLoaded": () => observed.loaded,
            "player.isLoaded": () => true,
            "auth.isLoggedIn": () => true,
            "world.isActionAvailable": () => observed.tfer,
            "world.getCells": () => observed.cells,
            "player.getCell": () => observed.live_cell,
            "player.getPad": () => observed.live_pad,
            ...Object.fromEntries(
              [
                "player.joinMap",
                "player.jump",
                "combat.cancelAutoAttack",
                "combat.cancelTarget",
                "world.isCellReady",
              ].map((method) => [
                method,
                (...args: string[]) => call(method, args),
              ]),
            ),
          },
        } as unknown as Window;
        for (const update of scenario.updates)
          setTimeout(() => {
            Object.assign(observed, update.patch);
            sync();
          }, update.at);
        const run = Effect.gen(function* () {
          const api = yield* makeFakeApi(target, state, scenario.commands.find((command) => command.delay_ms)?.delay_ms ?? 0);
          const request = scenario.request;
          if (request.kind === "join")
            return yield* api.player.joinMap(request.target!, {
              ...(request.cell === undefined ? {} : { cell: request.cell }),
              ...(request.pad === undefined ? {} : { pad: request.pad }),
            });
          if (request.kind === "jump")
            return yield* api.player.jumpToCell(request.cell!, request.pad);
          return yield* api.player.exitCombat();
        }).pipe(
          Effect.provideService(DiagnosticSink, { report: () => {} }),
          Effect.provideService(Random.Random, {
            nextDoubleUnsafe: () =>
              ((scenario.request.random_room ?? 10000) - 10000) / 90000,
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
        expect(await promise).toBe(scenario.outcome);
        expect(calls).toEqual(
          scenario.commands.map(({ method, args, at }) => ({
            method,
            args,
            at,
          })),
        );
        expect(index).toBe(scenario.commands.length);
        expect(finishedAt).toBe(scenario.finished_at);
      } finally {
        vi.useRealTimers();
      }
    });
  }
});
