import * as Schema from "effect/Schema";

export const SCRIPT_HTTP_MAX_BYTES = 8 * 1024 * 1024;

export const HttpErrorReasonSchema = Schema.Literals([
  "request",
  "redirect",
  "timeout",
  "aborted",
  "too-large",
  "body",
]);
export type HttpErrorReason = typeof HttpErrorReasonSchema.Type;

export const HttpFailureSchema = Schema.Struct({
  reason: HttpErrorReasonSchema,
  url: Schema.String,
  detail: Schema.String,
});
export type HttpFailure = typeof HttpFailureSchema.Type;

export class HttpError extends Schema.TaggedError<HttpError>()("HttpError", {
  ...HttpFailureSchema.fields,
  cause: Schema.optionalKey(Schema.Defect()),
}) {
  override get message(): string {
    return this.detail;
  }
}

export const HttpRequestPayloadSchema = Schema.Struct({
  requestId: Schema.Int.check(Schema.isGreaterThan(0)),
  url: Schema.String,
  method: Schema.String,
  headers: Schema.Record(Schema.String, Schema.String),
  body: Schema.optionalKey(Schema.Uint8Array),
  deadline: Schema.Finite,
  redirect: Schema.Literals(["follow", "manual", "error"]),
  maxRedirects: Schema.Int.check(Schema.isGreaterThanOrEqualTo(0)),
});
export type HttpRequestPayload = typeof HttpRequestPayloadSchema.Type;

export const HttpResponsePayloadSchema = Schema.Struct({
  body: Schema.Uint8Array,
  headers: Schema.Array(Schema.Tuple([Schema.String, Schema.String])),
  status: Schema.Int,
  statusText: Schema.String,
  url: Schema.String,
});
export type HttpResponsePayload = typeof HttpResponsePayloadSchema.Type;

export const HttpResultSchema = Schema.Union([
  Schema.Struct({ ok: Schema.Literal(true), value: HttpResponsePayloadSchema }),
  Schema.Struct({ ok: Schema.Literal(false), error: HttpFailureSchema }),
]);
export type HttpResult = typeof HttpResultSchema.Type;

export interface DesktopHttpBridge {
  readonly request: (payload: HttpRequestPayload) => Promise<HttpResult>;
  readonly cancel: (requestId: number) => Promise<void>;
}
