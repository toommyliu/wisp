import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { AccountScriptSettingsPatchSchema } from "@lucent/schemas/accountSettings";
import type { ScriptOptionsApi } from "@lucent/scripting-api/contract";
import type { HostStateService } from "../game/flash/state/HostState";
import { ScriptExecutionError } from "../game/scripting/ScriptRunnerErrors";

const decodePatch = Schema.decodeUnknownEffect(AccountScriptSettingsPatchSchema);
const invalidOptions = (cause: unknown) =>
  new ScriptExecutionError({
    cause,
    detail: "script.options.update requires valid runtime options.",
  });

export const makeScriptOptionsApi = (host: HostStateService): ScriptOptionsApi => ({
  get: () => host.read({ kind: "scriptOptions" }),
  reset: () =>
    host.command({ kind: "resetScriptOptions" }).pipe(
      Effect.flatMap((result) =>
        Option.isSome(result)
          ? Effect.void
          : Effect.die(new Error("Could not reset script options.")),
      ),
    ),
  update: (patch) =>
    decodePatch(patch).pipe(
      Effect.mapError(invalidOptions),
      Effect.flatMap((patch) => host.command({ kind: "updateScriptOptions", patch })),
      Effect.flatMap((result) =>
        Option.isSome(result)
          ? Effect.void
          : Effect.fail(invalidOptions("Host rejected script options.")),
      ),
    ),
});
