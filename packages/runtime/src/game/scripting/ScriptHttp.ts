import * as Effect from "effect/Effect";

import { HttpError, type DesktopHttpBridge } from "../../shared/http";
import type { HttpRequestOptions, ScriptHttpApi } from "@lucent/scripting-api/contract";
import { prepareRequest } from "./http/request";
import { executeRequest, requestFailure } from "./http/execute";
import type { ScriptAsyncScope } from "./scriptAsyncScope";

export const makeScriptHttpApi = Effect.fn("ScriptHttpApi.make")(function* (
  bridge: DesktopHttpBridge,
  scope: ScriptAsyncScope,
): Effect.fn.Return<ScriptHttpApi, HttpError> {
  let nextRequestId = 0;

  const request = Effect.fn("ScriptHttp.request")(function* (
    url: string | URL,
    options: HttpRequestOptions = {},
  ) {
    if (scope.signal.aborted) return yield* Effect.interrupt;
    const signal = options.signal;
    if (signal?.aborted)
      return yield* new HttpError({
        reason: "aborted",
        detail: "HTTP request was aborted.",
        url: String(url),
      });
    const payload = yield* Effect.try({
      try: () => prepareRequest(url, options, ++nextRequestId),
      catch: (cause) => requestFailure(cause, String(url)),
    });
    return yield* executeRequest(bridge, scope, signal, payload);
  });

  return Object.freeze({ HttpError, request });
});
