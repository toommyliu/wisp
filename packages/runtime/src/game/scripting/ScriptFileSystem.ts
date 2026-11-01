import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

import {
  FileSystemError as CoreFileSystemError,
  SCRIPT_FILE_SYSTEM_MAX_BYTES,
  type FileSystemErrorReason,
  type FileSystemOperation,
} from "@lucent/schemas/filesystem";
import type {
  DesktopFileSystemBridge,
  DesktopFileSystemResult,
} from "../../shared/desktopBridge";
import type { ScriptFileSystemApi } from "@lucent/scripting-api/contract";
import type { ScriptAsyncScope } from "./scriptAsyncScope";

const makeError = (
  operation: FileSystemOperation,
  reason: FileSystemErrorReason,
  path?: string,
): CoreFileSystemError =>
  new CoreFileSystemError({
    operation,
    reason,
    ...(path === undefined ? {} : { path }),
  });

const isJson = Schema.is(Schema.Json);
const tooLarge = (text: string): boolean => {
  let bytes = 0;
  for (let index = 0; index < text.length; index++) {
    const code = text.charCodeAt(index);
    if (code < 0x80) bytes++;
    else if (code < 0x800) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff && index + 1 < text.length &&
      text.charCodeAt(index + 1) >= 0xdc00 && text.charCodeAt(index + 1) <= 0xdfff) {
      bytes += 4;
      index++;
    } else bytes += 3;
    if (bytes > SCRIPT_FILE_SYSTEM_MAX_BYTES) return true;
  }
  return false;
};

export const makeScriptFileSystemApi = (
  bridge: DesktopFileSystemBridge,
  scope: ScriptAsyncScope,
): ScriptFileSystemApi => {
  const request = <A>(
    operation: FileSystemOperation,
    path: string | undefined,
    invoke: (signal: AbortSignal) => Promise<DesktopFileSystemResult<A>>,
    validate?: () => CoreFileSystemError | undefined,
  ): Effect.Effect<A, CoreFileSystemError> =>
    Effect.suspend(() => {
      if (scope.signal.aborted) {
        return Effect.fail(makeError(operation, "session-closed", path));
      }
      const validationError = validate?.();
      if (validationError !== undefined) {
        return Effect.fail(validationError);
      }
      return Effect.tryPromise({
        try: invoke,
        catch: () => makeError(operation, "unavailable", path),
      }).pipe(
        Effect.flatMap((result) =>
          result.ok
            ? Effect.succeed(result.value)
            : Effect.fail(
                makeError(
                  result.error.operation,
                  result.error.reason,
                  result.error.path,
                ),
              ),
        ),
      );
    });

  const api: ScriptFileSystemApi = {
    FileSystemError: CoreFileSystemError,
    exists: (path) => request("exists", path, (signal) => bridge.exists(path, signal)),
    list: (path) =>
      request("list", path, (signal) => bridge.list(path, signal)).pipe(
        Effect.map((entries) =>
          Object.freeze(entries.map((entry) => Object.freeze({ ...entry }))),
        ),
      ),
    readJson: (path) => request("read-json", path, (signal) => bridge.readJson(path, signal)),
    readText: (path) => request("read-text", path, (signal) => bridge.readText(path, signal)),
    remove: (path) => request("remove", path, (signal) => bridge.remove(path, signal)),
    writeJson: (path, value) =>
      request(
        "write-json",
        path,
        (signal) => bridge.writeJson(path, value, signal),
        () => {
          if (!isJson(value)) {
            return makeError("write-json", "not-json-serializable", path);
          }
          const source = `${JSON.stringify(value, null, 2)}\n`;
          return tooLarge(source)
            ? makeError("write-json", "too-large", path)
            : undefined;
        },
      ),
    writeText: (path, contents) =>
      request(
        "write-text",
        path,
        (signal) => bridge.writeText(path, contents, signal),
        () =>
          tooLarge(contents)
            ? makeError("write-text", "too-large", path)
            : undefined,
      ),
  };
  return Object.freeze(api);
};
