import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";

import type { ApiService } from "../../flash/api/Api";
import type {
  ScriptEventSelector,
  ScriptEventsApi,
  ScriptEventsOn,
  ScriptWaitForEvent,
} from "@lucent/scripting-api/contract";
import type { ScriptAsyncScope } from "../scriptAsyncScope";
import { notifyScriptCallbackFailure } from "./Callbacks";

export const makeScriptEventsApi = (
  events: ApiService["events"],
  scope: ScriptAsyncScope,
  failCause: (cause: Cause.Cause<unknown>) => Effect.Effect<void>,
): ScriptEventsApi => {
  const on = ((
    selector: ScriptEventSelector | undefined,
    handler: Parameters<ScriptEventsOn>[1],
  ) => {
    return events
      .onProjection(selector, notifyScriptCallbackFailure(handler, failCause))
      .pipe(Effect.tap((dispose) => scope.addCleanup(dispose)));
  }) as ScriptEventsOn;

  const once = events.onceProjection as ScriptWaitForEvent;

  return Object.freeze({
    on,
    once,
  });
};
