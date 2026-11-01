import * as Effect from "effect/Effect";
import * as Duration from "effect/Duration";

import type {
  ScriptInputValue,
  ScriptInputValues,
} from "@lucent/schemas/scriptInputs";
import type {
  RoomPolicy,
  ScriptRuntimeApi,
  ScriptOptionsApi,
} from "@lucent/scripting-api/contract";
import type { ScriptAsyncScope } from "./scriptAsyncScope";
import type { ScriptDialogSource, ScriptDialogsShape } from "./ScriptDialogs";
import {
  makeScriptExitSignal,
  ScriptExecutionError,
  ScriptStopSignal,
} from "./ScriptRunnerErrors";

export const snapshotRoomPolicy = (policy: RoomPolicy): RoomPolicy => ({
  ...policy,
});

const snapshotScriptInputValue = (value: ScriptInputValue): ScriptInputValue =>
  Array.isArray(value) ? [...value] : value;

const snapshotScriptInputValues = (
  values: ScriptInputValues,
): ScriptInputValues => {
  const snapshot: Record<string, ScriptInputValue> = {};
  for (const [key, value] of Object.entries(values)) {
    snapshot[key] = snapshotScriptInputValue(value);
  }
  return snapshot;
};

export interface ScriptRuntimeApiOptions {
  readonly dialogs: Pick<
    ScriptDialogsShape,
    "alert" | "beep" | "confirm" | "prompt"
  >;
  readonly scriptOptions: ScriptOptionsApi;
  readonly getInputValues: () => ScriptInputValues;
  readonly log: (...values: unknown[]) => void;
  readonly scope: ScriptAsyncScope;
  readonly source: ScriptDialogSource;
}

export const makeScriptRuntimeApi = (
  options: ScriptRuntimeApiOptions,
): ScriptRuntimeApi => {
  const source = { ...options.source };
  const script: ScriptRuntimeApi = {
    alert: (message) => options.dialogs.alert(source, message),
    beep: (times = 1) =>
      Number.isFinite(times)
        ? options.dialogs.beep(Math.max(1, Math.floor(times)))
        : Effect.fail(
            new ScriptExecutionError({
              detail: "script.beep requires a finite number of repetitions.",
            }),
          ),
    confirm: (message) => options.dialogs.confirm(source, message),
    exit: (exitOptions) => Effect.fail(makeScriptExitSignal(exitOptions)),
    inputs: Object.freeze({
      get: (key: string) =>
        Effect.sync(() => {
          const value = options.getInputValues()[key];
          return value === undefined
            ? undefined
            : snapshotScriptInputValue(value);
        }),
      getAll: () =>
        Effect.sync(() => snapshotScriptInputValues(options.getInputValues())),
    }),
    log: (...values) => Effect.sync(() => options.log(...values)),
    options: Object.freeze(options.scriptOptions),
    prompt: (message, placeholder) =>
      options.dialogs.prompt(source, message, placeholder),
    signal: options.scope.signal,
    sleep: (duration) =>
      Effect.try({
        try: () => Duration.toMillis(duration),
        catch: (cause) =>
          new ScriptExecutionError({
            cause,
            detail: "Invalid duration.",
          }),
      }).pipe(
        Effect.flatMap((milliseconds) =>
          Number.isFinite(milliseconds) && milliseconds >= 0
            ? Effect.sleep(duration)
            : Effect.fail(
                new ScriptExecutionError({
                  detail: "Duration must be non-negative and finite.",
                }),
              ),
        ),
      ),
    stop: (reason) =>
      Effect.fail(new ScriptStopSignal(reason === undefined ? {} : { reason })),
  };
  return Object.freeze(script);
};
