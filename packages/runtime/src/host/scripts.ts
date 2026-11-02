import type { ScriptOutcome } from "@lucent/game/protocol";
import type { ScriptInputValues } from "@lucent/schemas/scriptInputs";
import type * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";
import type * as Exit from "effect/Exit";

import { makeApi } from "../game/flash/api/Api";
import { makeBridge } from "../game/flash/bridge/Bridge";
import { makeGateway } from "../game/flash/bridge/Gateway";
import { makeHostState } from "../game/flash/state/HostState";
import { makeScriptRuntimeServices } from "../game/scripting/api/Services";
import {
  makeScriptBuiltinModules,
  type ScriptBuiltinModules,
} from "../game/scripting/ScriptBuiltinModules";
import { makeScriptFileSystemApi } from "../game/scripting/ScriptFileSystem";
import { makeScriptHttpApi } from "../game/scripting/ScriptHttp";
import { ScriptLoadError } from "../game/scripting/ScriptRunnerErrors";
import {
  makeScriptRuntimeApi,
  snapshotRoomPolicy,
} from "../game/scripting/ScriptRuntime";
import {
  makeScriptAsyncScope,
  type ScriptAsyncScope,
} from "../game/scripting/scriptAsyncScope";
import { readScriptEntry } from "../game/scripting/scriptEntry";
import { makeAutoReloginService } from "./autoRelogin";
import { makeAutoZoneService } from "./autoZone";
import { makeHostDialogs } from "./dialogs";
import { makeEnvironmentService } from "./environment";
import { makeFileSystemBridge } from "./filesystem";
import { makeHttpBridge } from "./http";
import { hideInternals } from "./loop";
import { failedOutcome, scriptOutcome } from "./outcome";
import { makeScriptOptionsApi } from "./scriptOptions";
import { unavailableArmy, unavailableEnvironment } from "./services";

export type ScriptLoad = (builtins: ScriptBuiltinModules) => unknown;

type Ending =
  | { readonly kind: "finished"; readonly exit: Exit.Exit<unknown, unknown> }
  | { readonly kind: "stopped"; readonly reason: string | null }
  | { readonly kind: "failed"; readonly cause: Cause.Cause<unknown> };

const host = __host;
const swf = { swf: window.swf };
let end: (ending: Ending) => void = () => undefined;

const outcome = (ending: Ending): ScriptOutcome => {
  switch (ending.kind) {
    case "finished":
      return scriptOutcome(ending.exit);
    case "stopped":
      return { kind: "externally-stopped", reason: ending.reason };
    case "failed":
      return failedOutcome(ending.cause);
  }
};

const run = (
  load: ScriptLoad,
  name: string,
  inputs: ScriptInputValues,
  scope: ScriptAsyncScope,
) =>
  Effect.gen(function* () {
    const bridge = yield* makeBridge(swf);
    const gateway = yield* makeGateway(bridge);
    const state = yield* makeHostState(host);
    const api = yield* makeApi(bridge, gateway, state);
    yield* Effect.addFinalizer(() => scope.close);
    const scriptOptions = makeScriptOptionsApi(state);
    let inputValues: ScriptInputValues = {};
    const modules = makeScriptBuiltinModules({
      autoRelogin: makeAutoReloginService(host),
      autoZone: makeAutoZoneService(host),
      bridge,
      failCause: (cause) => Effect.sync(() => end({ kind: "failed", cause })),
      fileSystem: makeScriptFileSystemApi(makeFileSystemBridge(host), scope),
      http: yield* makeScriptHttpApi(makeHttpBridge(host), scope),
      roomPolicy: scriptOptions
        .get()
        .pipe(Effect.map((options) => snapshotRoomPolicy(options.roomPolicy))),
      scope,
      script: makeScriptRuntimeApi({
        dialogs: makeHostDialogs(host),
        scriptOptions,
        getInputValues: () => inputValues,
        log: (...values) => console.log("[script]", ...values),
        scope,
        source: { sourceName: name.trim() === "" ? "script" : name },
      }),
      services: makeScriptRuntimeServices(
        api,
        unavailableArmy,
        makeEnvironmentService(host, unavailableEnvironment),
      ),
    });
    hideInternals();
    const namespace = yield* Effect.try({
      try: () => load(modules),
      catch: (cause) =>
        new ScriptLoadError({
          detail: cause instanceof Error ? cause.message : String(cause),
          cause,
        }),
    });
    const { main, inputValues: values } = yield* readScriptEntry(
      namespace,
      name,
      inputs,
    );
    inputValues = values;
    const exit = yield* Effect.exit(
      Effect.gen(function* () {
        return yield* main() as Generator<
          Effect.Effect<any, any, never>,
          unknown,
          any
        >;
      }),
    );
    end({ kind: "finished", exit });
  }).pipe(Effect.scoped);

export const start = (
  load: ScriptLoad,
  name: string,
  inputsJson: string,
): Promise<ScriptOutcome> => {
  const root = new AbortController();
  const scope = makeScriptAsyncScope();
  let ending: Ending | undefined;
  end = (next) => {
    if (ending !== undefined) return;
    ending = next;
    if (next.kind === "finished") return;
    scope.controller.abort();
    root.abort();
  };
  return Effect.runPromiseExit(
    run(load, name, JSON.parse(inputsJson) as ScriptInputValues, scope),
    { signal: root.signal },
  ).then((exit) => outcome(ending ?? { kind: "finished", exit }));
};

export const stop = (reason?: string): Promise<void> => {
  end({ kind: "stopped", reason: reason ?? null });
  return Promise.resolve();
};
