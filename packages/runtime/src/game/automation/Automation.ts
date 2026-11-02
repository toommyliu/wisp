import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as FiberMap from "effect/FiberMap";
import * as Layer from "effect/Layer";
import * as Stream from "effect/Stream";

import { Api } from "../flash/api/Api";
import { makeAutoAttack } from "./AutoAttack";
import type { AutoAttackState } from "./AutoAttack";

export const makeAutomation = Effect.gen(function* () {
  const api = yield* Api;
  const scope = yield* Effect.scope;
  const fibers = yield* FiberMap.make<string>();
  const runFork = Effect.runForkWith(yield* Effect.context<never>());
  const autoAttack = yield* makeAutoAttack(api, fibers);

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

  const observeAutoAttack = (listener: (state: AutoAttackState) => void) =>
    observe(autoAttack.changes, listener);

  const autoAttackApi = {
    ...autoAttack,
    onState: observeAutoAttack,
  };

  return {
    autoAttack: autoAttackApi,
  };
});

export type AutomationService = Effect.Success<typeof makeAutomation>;

export class Automation extends Context.Service<
  Automation,
  AutomationService
>()("lucent/renderer/automation/Automation") {}

export const layer = Layer.effect(Automation, makeAutomation);
