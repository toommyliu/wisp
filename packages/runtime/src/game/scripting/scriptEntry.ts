import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

import {
  ScriptInputsDefinitionSchema,
  validateScriptInputValues,
  type ScriptInputValues,
} from "@lucent/schemas/scriptInputs";
import type { ScriptMain } from "@lucent/scripting-api/contract";
import { ScriptLoadError } from "./ScriptRunnerErrors";

export interface ScriptEntry {
  readonly main: ScriptMain;
  readonly inputValues: ScriptInputValues;
}

const isGeneratorFunction = (value: unknown): value is ScriptMain =>
  typeof value === "function" &&
  value.constructor?.name === "GeneratorFunction";

export const readScriptEntry = Effect.fn("ScriptEntry.read")(function* (
  namespace: unknown,
  name: string,
  suppliedValues: ScriptInputValues,
): Effect.fn.Return<ScriptEntry, ScriptLoadError> {
  const exports =
    typeof namespace === "object" && namespace !== null ? namespace : {};
  const main: unknown = Reflect.get(exports, "default");
  if (!isGeneratorFunction(main)) {
    return yield* new ScriptLoadError({
      detail: `${name} must export a generator function as its default export.`,
    });
  }
  const inputs: unknown = Reflect.get(exports, "inputs");
  if (inputs === undefined) return { main, inputValues: {} };
  const definition = yield* Schema.decodeUnknownEffect(
    ScriptInputsDefinitionSchema,
  )(inputs).pipe(
    Effect.mapError(
      (cause) =>
        new ScriptLoadError({
          detail: `${name} has an invalid inputs export.`,
          cause,
        }),
    ),
  );
  const validation = validateScriptInputValues(definition, suppliedValues);
  if (validation.status === "missing-required") {
    return yield* new ScriptLoadError({
      detail: `${name} is missing required inputs: ${validation.fieldKeys.join(", ")}.`,
    });
  }
  return { main, inputValues: validation.values };
});
