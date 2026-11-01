import * as Schema from "effect/Schema";

import type { ScriptExitOptions } from "@lucent/scripting-api/contract";

export class ScriptLoadError extends Schema.TaggedError<ScriptLoadError>()(
  "ScriptLoadError",
  {
    detail: Schema.String,
    cause: Schema.optionalKey(Schema.Defect()),
  },
) {
  override get message(): string {
    return this.detail;
  }
}

export class ScriptExecutionError extends Schema.TaggedError<ScriptExecutionError>()(
  "ScriptExecutionError",
  {
    detail: Schema.String,
    cause: Schema.optionalKey(Schema.Defect()),
  },
) {
  override get message(): string {
    return this.detail;
  }
}

export class ScriptNotReadyError extends Schema.TaggedError<ScriptNotReadyError>()(
  "ScriptNotReadyError",
  {
    detail: Schema.String,
    missing: Schema.Array(Schema.String),
  },
) {
  override get message(): string {
    return this.detail;
  }
}

export class ScriptStopSignal extends Schema.TaggedError<ScriptStopSignal>()(
  "ScriptStopSignal",
  {
    reason: Schema.optionalKey(Schema.String),
  },
) {
  override get message(): string {
    return this.reason ?? "Script stopped.";
  }
}

export interface ScriptExitRequest {
  readonly closeClient: boolean;
  readonly logout: boolean;
}

const scriptExitRequests = new WeakMap<ScriptStopSignal, ScriptExitRequest>();

export const makeScriptExitSignal = (
  options?: ScriptExitOptions,
): ScriptStopSignal => {
  const signal = new ScriptStopSignal({ reason: "Requested by the script" });
  const request: ScriptExitRequest = {
    closeClient: options?.closeClient === true,
    logout: options?.logout === true,
  };
  // Keep an explicit marker even when no actions were requested. The runner
  // must distinguish script.exit() from script.stop().
  scriptExitRequests.set(signal, request);
  return signal;
};

export const getScriptExitRequest = (
  signal: ScriptStopSignal,
): ScriptExitRequest | undefined => scriptExitRequests.get(signal);
