import * as Duration from "effect/Duration";

import { HttpError, type HttpRequestPayload } from "../../../shared/http";
import type { HttpRequestOptions } from "@lucent/scripting-api/contract";

const encoder = new TextEncoder();
const DEFAULT_TIMEOUT_MS = 20_000;

export const prepareRequest = (
  input: string | URL,
  options: HttpRequestOptions,
  requestId: number,
): HttpRequestPayload => {
  const startedAt = Date.now();
  const url = new URL(input);
  if (
    !["http:", "https:"].includes(url.protocol) ||
    url.username !== "" ||
    url.password !== ""
  )
    throw new Error(
      "Expected an absolute HTTP or HTTPS URL without embedded credentials.",
    );
  const method = (options.method ?? "GET").toUpperCase();
  if (!/^[!#$%&'*+.^_`|~0-9A-Z-]+$/.test(method))
    throw new Error("Invalid HTTP method.");
  if ((method === "GET" || method === "HEAD") && options.body !== undefined)
    throw new Error(`${method} requests cannot have a body.`);
  const maxRedirects = options.maxRedirects ?? 5;
  if (!Number.isSafeInteger(maxRedirects) || maxRedirects < 0)
    throw new Error("maxRedirects must be a nonnegative integer.");
  const redirect = options.redirect ?? "follow";
  if (!["follow", "manual", "error"].includes(redirect))
    throw new Error("Invalid redirect mode.");
  const timeoutMs =
    options.timeout === undefined
      ? DEFAULT_TIMEOUT_MS
      : Duration.toMillis(options.timeout);
  if (
    !Number.isFinite(timeoutMs) ||
    timeoutMs < 0 ||
    timeoutMs > 2_147_483_647 ||
    (typeof options.timeout === "number" && Number.isNaN(options.timeout))
  )
    throw new Error(
      "timeout must be a nonnegative duration of at most 2147483647 milliseconds.",
    );
  if (timeoutMs === 0)
    throw new HttpError({
      reason: "timeout",
      detail: "HTTP request timed out before starting.",
      url: url.href,
    });

  const headers = new Headers(options.headers);
  let body: Uint8Array | undefined;
  if (typeof options.body === "string") {
    body = encoder.encode(options.body);
    if (!headers.has("content-type"))
      headers.set("content-type", "text/plain;charset=UTF-8");
  } else if (options.body instanceof URLSearchParams) {
    body = encoder.encode(options.body.toString());
    if (!headers.has("content-type"))
      headers.set(
        "content-type",
        "application/x-www-form-urlencoded;charset=UTF-8",
      );
  } else if (options.body instanceof ArrayBuffer) {
    body = new Uint8Array(options.body);
  } else if (options.body instanceof Uint8Array) {
    body = options.body;
  } else if (options.body !== undefined) {
    throw new Error("Unsupported HTTP body.");
  }
  return {
    requestId,
    url: url.href,
    method,
    headers: Object.fromEntries(headers),
    ...(body === undefined ? {} : { body }),
    deadline: startedAt + timeoutMs,
    redirect,
    maxRedirects,
  };
};
