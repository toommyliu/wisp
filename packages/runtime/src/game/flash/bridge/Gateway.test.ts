import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as Option from "effect/Option";
import * as PubSub from "effect/PubSub";
import * as Stream from "effect/Stream";

import { makeBridge } from "./Bridge";
import { makeGateway } from "./Gateway";

const swfTarget = {
  swf: {
    "flash.sendClientPacket": () => undefined,
  },
} as unknown as Pick<Window, "swf">;

describe("Gateway", () => {
  it.effect("owns callbacks until disposal", () => {
    const target = {} as Window;

    return Effect.gen(function* () {
      yield* Effect.scoped(
        Effect.gen(function* () {
          yield* makeGateway(yield* makeBridge(swfTarget), target);
          expect(target.packetFromClient).toBeTypeOf("function");
          expect(target.onProjectionEvent).toBeTypeOf("function");
        }),
      );

      expect(target.packetFromClient).toBeUndefined();
      expect(target.onProjectionEvent).toBeUndefined();
    });
  });

  it.effect(
    "delivers a loader callback before the projection events it produced",
    () => {
      const target = {} as Window;

      return Effect.scoped(
        Effect.gen(function* () {
          const gateway = yield* makeGateway(yield* makeBridge(swfTarget), target);
          const packets = yield* gateway.subscribePackets;
          const events = yield* gateway.subscribeEvents;
          const packetsBeforeEvent: Array<readonly [string, number]> = [];
          yield* gateway.start((event) =>
            PubSub.remaining(packets).pipe(
              Effect.map((published) => {
                packetsBeforeEvent.push([event.type, published]);
              }),
            ),
          );

          target.onExtensionResponse?.(
            JSON.stringify({
              dataObj: { areaId: 5, areaName: "battleon-1", cmd: "moveToArea" },
              type: "json",
            }),
          );
          target.onProjectionEvent?.({
            map: { id: 5, name: "battleon", roomNumber: 1 },
            type: "join-map",
          });

          expect(yield* PubSub.take(events)).toEqual({
            map: { id: 5, name: "battleon", roomNumber: 1 },
            type: "join-map",
          });
          expect(packetsBeforeEvent).toEqual([["join-map", 1]]);
          expect((yield* PubSub.take(packets)).command).toBe("moveToArea");
        }),
      );
    },
  );

  it.effect(
    "publishes raw packets even when their envelope is unsupported",
    () => {
      const target = {} as Window;

      return Effect.scoped(
        Effect.gen(function* () {
          const gateway = yield* makeGateway(yield* makeBridge(swfTarget), target);
          yield* gateway.start();
          const capture = yield* gateway.rawPackets.pipe(
            Stream.runHead,
            Effect.forkScoped,
          );
          yield* Effect.yieldNow;

          target.packetFromClient?.("<msg t='sys'></msg>");

          expect(Option.getOrNull(yield* Fiber.join(capture))).toEqual({
            direction: "client",
            raw: "<msg t='sys'></msg>",
          });
        }),
      );
    },
  );
});
