import type { DesktopHttpBridge, HttpResponsePayload, HttpResult } from "../shared/http";
import { requestIo } from "./io";

type Host = Pick<typeof __host, "requestIo" | "cancelIo" | "takeIoBody">;
type Response = Omit<HttpResponsePayload, "body"> & { readonly body: string };
type Result = { readonly ok: true; readonly value: Response } | Extract<HttpResult, { ok: false }>;

export const makeHttpBridge = (host: Host): DesktopHttpBridge => {
  const requests = new Map<number, AbortController>();
  return {
    request: async (payload) => {
      const controller = new AbortController();
      requests.set(payload.requestId, controller);
      try {
        const { body, ...metadata } = payload;
        const result = await requestIo<Result>(host, { service: "http", ...metadata }, controller.signal, body);
        return result.ok
          ? { ok: true, value: { ...result.value, body: new Uint8Array(host.takeIoBody(result.value.body)) } }
          : result;
      } finally {
        requests.delete(payload.requestId);
      }
    },
    cancel: async (id) => { requests.get(id)?.abort(); },
  };
};
