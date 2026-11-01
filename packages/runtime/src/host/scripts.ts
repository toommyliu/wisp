import type { ScriptOutcome } from "@lucent/game/protocol";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as ManagedRuntime from "effect/ManagedRuntime";

import { scriptApiLayer } from "../game/flash/apiLayer";
import { ScriptDialogs } from "../game/scripting/ScriptDialogs";
import {
  ScriptRunner,
  layer as scriptRunnerLayer,
  type ScriptSource,
} from "../game/scripting/ScriptRunner";
import { ScriptServices } from "../game/scripting/ScriptServices";
import { makeAutoReloginService } from "./autoRelogin";
import { makeAutoZoneService } from "./autoZone";
import { makeFileSystemBridge } from "./filesystem";
import { makeHostDialogs } from "./dialogs";
import { hideInternals } from "./loop";
import { unavailableScriptServices } from "./services";

const api = ManagedRuntime.make(scriptApiLayer);
api.runSync(Effect.void);
const runtime = ManagedRuntime.make(
  scriptRunnerLayer.pipe(
    Layer.provide(
      Layer.mergeAll(
        scriptApiLayer,
        Layer.succeed(ScriptServices, {
          ...unavailableScriptServices,
          fileSystem: makeFileSystemBridge(__host),
          autoZone: makeAutoZoneService(__host),
          autoRelogin: makeAutoReloginService(__host),
        }),
        Layer.succeed(ScriptDialogs, makeHostDialogs(__host)),
      ),
    ),
  ),
  { memoMap: api.memoMap },
);
runtime.runSync(Effect.void);

export const start = (
  load: ScriptSource["load"],
  name: string,
  inputsJson: string,
): Promise<ScriptOutcome> => {
  hideInternals();
  return runtime.runPromise(
    Effect.gen(function* () {
      const runner = yield* ScriptRunner;
      const handle = yield* runner.start(
        { name, load },
        JSON.parse(inputsJson),
      );
      const outcome = yield* handle.terminal;
      switch (outcome.kind) {
        case "completed":
          return { kind: "completed" };
        case "failed":
          return {
            kind: "failed",
            message: outcome.status.message,
            details: outcome.status.detailsText ?? null,
          };
        case "script-exited":
          return {
            kind: "script-exited",
            closeClient: outcome.closeClient,
            logout: outcome.logout,
          };
        case "script-stopped":
        case "externally-stopped":
          return { kind: outcome.kind, reason: outcome.status.reason ?? null };
      }
    }),
  );
};

export const stop = (reason?: string): Promise<void> =>
  runtime.runPromise(
    Effect.flatMap(ScriptRunner, (runner) => runner.stop(reason)).pipe(
      Effect.asVoid,
    ),
  );
