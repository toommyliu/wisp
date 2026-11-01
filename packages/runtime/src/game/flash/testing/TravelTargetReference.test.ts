import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Random from "effect/Random";
import { parseMapTarget, minimumRandomPrivateRoom, maximumRoom } from "./TravelTargetReference";
const withRandomValue = <A>(effect: Effect.Effect<A>, value: number) => effect.pipe(Effect.provideService(Random.Random, { nextDoubleUnsafe: () => value, nextIntUnsafe: () => 0 }));
describe("travel target reference", () => {
  it.effect("distinguishes public rooms from exact rooms", () =>
    Effect.gen(function* () {
      expect(yield* parseMapTarget(" battleon ")).toEqual({
        map: "battleon",
        name: "battleon",
        requireExactRoom: false,
      });

      expect(yield* parseMapTarget("battleon-1")).toEqual({
        map: "battleon-1",
        name: "battleon",
        requireExactRoom: false,
        roomNumber: 1,
      });
      expect(yield* parseMapTarget("battleon-999")).toEqual({
        map: "battleon-999",
        name: "battleon",
        requireExactRoom: false,
        roomNumber: 999,
      });

      expect(yield* parseMapTarget("battleon-1000")).toEqual({
        map: "battleon-1000",
        name: "battleon",
        requireExactRoom: false,
        roomNumber: 1_000,
      });
      expect(yield* parseMapTarget("battleon-1001")).toEqual({
        map: "battleon-1001",
        name: "battleon",
        requireExactRoom: true,
        roomNumber: 1_001,
      });
      expect(yield* parseMapTarget("battleon-99999")).toEqual({
        map: "battleon-99999",
        name: "battleon",
        requireExactRoom: true,
        roomNumber: maximumRoom,
      });
      expect(yield* parseMapTarget("battleon-100000")).toEqual({
        map: "battleon-100000",
        name: "battleon",
        requireExactRoom: false,
        roomNumber: 100_000,
      });
    }),
  );

  it.effect("replaces invalid room suffixes with a private room", () =>
    Effect.gen(function* () {
      for (const map of ["battleon-private", "battleon-0", "battleon-1.5"]) {
        expect(yield* withRandomValue(parseMapTarget(map), 0)).toEqual({
          map: `battleon-${minimumRandomPrivateRoom}`,
          name: "battleon",
          requireExactRoom: true,
          roomNumber: minimumRandomPrivateRoom,
        });
      }
    }),
  );


});
