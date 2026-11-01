import { describe, expect, it } from "@effect/vitest";
import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";

import { makeScriptArmyApi } from "../game/scripting/api/Army";
import { makeScriptAsyncScope } from "../game/scripting/scriptAsyncScope";
import { makeScriptRuntimeApi } from "../game/scripting/ScriptRuntime";
import {
  unavailableScriptDialogs,
  unavailableScriptServices,
} from "./services";

const failure = <A, E>(effect: Effect.Effect<A, E>) =>
  effect.pipe(
    Effect.matchCause({
      onFailure: (cause) => {
        const error = Cause.squash(cause);
        return error instanceof Error ? error.message : String(error);
      },
      onSuccess: () => "unexpected success",
    }),
  );

describe("unavailable host services", () => {
  it.effect("reports the errors exposed to scripts", () =>
    Effect.gen(function* () {
      const scope = makeScriptAsyncScope();
      yield* Effect.addFinalizer(() => scope.close);
      const services = unavailableScriptServices;
      const army = makeScriptArmyApi(services.army, scope, () => Effect.void);
      const http = yield* services.http();
      const script = makeScriptRuntimeApi({
        dialogs: unavailableScriptDialogs,
        getOptions: () =>
          Effect.succeed({
            restartAfterReconnect: false,
            roomPolicy: { kind: "public" },
            safeStartStop: true,
          }),
        getInputValues: () => ({}),
        log: () => undefined,
        scope,
        setOptions: (update) =>
          Effect.succeed(
            update({
              restartAfterReconnect: false,
              roomPolicy: { kind: "public" },
              safeStartStop: true,
            }),
          ),
        source: { sourceName: "farm.js" },
      });
      expect(yield* failure(army.start("farm"))).toBe(
        "The army service is not available yet.",
      );
      expect(yield* failure(services.environment.syncToAll())).toBe(
        "The environment service is not available yet.",
      );
      expect(yield* failure(http.request("https://example.com"))).toBe(
        "The http service is not available yet.",
      );
      expect(yield* failure(script.alert("Done"))).toBe(
        "The dialogs service is not available yet.",
      );
      expect(yield* failure(script.beep())).toBe(
        "The beep service is not available yet.",
      );
    }),
  );
});
