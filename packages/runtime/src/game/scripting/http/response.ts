import * as Effect from "effect/Effect";

import {
  HttpError,
  type HttpResponsePayload,
} from "../../../shared/http";
import type { HttpResponse } from "@lucent/scripting-api/contract";

export const makeHttpResponse = (
  payload: HttpResponsePayload,
): HttpResponse => {
  let bytes: Uint8Array | undefined = payload.body;
  const url = payload.url;
  const read = <Value>(
    decode: (body: Uint8Array) => Value,
  ): Effect.Effect<Value, HttpError> =>
    Effect.try({
      try: () => {
        if (bytes === undefined)
          throw new Error("HTTP response body has already been consumed.");
        const body = bytes;
        bytes = undefined;
        return decode(body);
      },
      catch: (cause) =>
        new HttpError({
          reason: "body",
          url,
          detail:
            cause instanceof Error
              ? cause.message
              : "HTTP response body could not be read.",
          cause,
        }),
    });
  return Object.freeze({
    status: payload.status,
    statusText: payload.statusText,
    ok: payload.status >= 200 && payload.status < 300,
    url,
    headers: new Headers(payload.headers.map(([name, value]) => [name, value])),
    get bodyUsed() {
      return bytes === undefined;
    },
    json: () => read((body): unknown => JSON.parse(new TextDecoder().decode(body))),
    text: () => read((body) => new TextDecoder().decode(body)),
    arrayBuffer: () => read((body) => new Uint8Array(body).buffer),
  });
};
