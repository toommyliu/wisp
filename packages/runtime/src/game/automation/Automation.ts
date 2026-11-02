import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as FiberMap from "effect/FiberMap";
import * as Layer from "effect/Layer";
import * as Stream from "effect/Stream";

import { FollowerRpcs } from "../../shared/gameRendererRpc";
import { Api } from "../flash/api/Api";
import { makeDesktopFollowerPort, makeFollower } from "./Follower";
import type { FollowerState } from "@lucent/schemas/follower";

export const makeAutomation = Effect.gen(function* () {
  const api = yield* Api;
  const scope = yield* Effect.scope;
  const fibers = yield* FiberMap.make<string>();
  const runFork = Effect.runForkWith(yield* Effect.context<never>());
  const follower = yield* makeFollower(api, fibers, makeDesktopFollowerPort());

  const observe = <S>(
    changes: Stream.Stream<S>,
    listener: (state: S) => void,
  ) =>
    changes.pipe(
      Stream.runForEach((state) => Effect.sync(() => listener(state))),
      Effect.forkIn(scope),
      Effect.map((fiber) => () => {
        runFork(Fiber.interrupt(fiber));
      }),
    );

  const observeFollower = (listener: (state: FollowerState) => void) =>
    observe(follower.changes, listener);

  const followerApi = {
    ...follower,
    onState: observeFollower,
  };

  return {
    follower: followerApi,
  };
});

export type AutomationService = Effect.Success<typeof makeAutomation>;

export class Automation extends Context.Service<
  Automation,
  AutomationService
>()("lucent/renderer/automation/Automation") {}

export const layer = Layer.effect(Automation, makeAutomation);

export const followerRpcHandlers = FollowerRpcs.toLayer(
  Effect.gen(function* () {
    const { follower } = yield* Automation;
    return FollowerRpcs.of({
      FollowerConfigure: (config) => follower.configure(config),
      FollowerGetState: () => follower.getState(),
      FollowerMe: () => follower.me(),
      FollowerStart: (config) => follower.startWithSavedProfiles(config),
      FollowerStop: () => follower.stop(),
    });
  }),
);
