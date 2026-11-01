import * as Context from "effect/Context";

import type { DesktopFileSystemBridge } from "../../shared/desktopBridge";
import type * as Effect from "effect/Effect";

import type { HttpError } from "../../shared/http";
import type { ArmyApiRuntimeShape } from "../army/Army";
import type { EnvironmentShape } from "../environment/Environment";
import type {
  ScriptAutoReloginApi,
  ScriptAutoZoneApi,
  ScriptHttpApi,
} from "@lucent/scripting-api/contract";
import type { ScriptAsyncScope } from "./scriptAsyncScope";

export interface ScriptServicesShape {
  readonly army: ArmyApiRuntimeShape;
  readonly environment: EnvironmentShape;
  readonly autoRelogin: ScriptAutoReloginApi;
  readonly autoZone: ScriptAutoZoneApi;
  readonly fileSystem: DesktopFileSystemBridge;
  readonly http: (
    scope: ScriptAsyncScope,
  ) => Effect.Effect<ScriptHttpApi, HttpError>;
}

export class ScriptServices extends Context.Service<
  ScriptServices,
  ScriptServicesShape
>()("lucent/game/scripting/ScriptServices") {}
