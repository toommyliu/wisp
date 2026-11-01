import * as Schema from "effect/Schema";
import type { DesktopFileSystemBridge, DesktopFileSystemResult } from "../shared/desktopBridge";
import { requestIo } from "./io";

const isJson = Schema.is(Schema.Json);

export const makeFileSystemBridge = (
  host: Pick<typeof __host, "requestIo" | "cancelIo">,
): DesktopFileSystemBridge => {
  const request = <A>(operation: string, path: string | undefined, signal?: AbortSignal, contents?: string) =>
    requestIo<DesktopFileSystemResult<A>>(host, { service: "filesystem", operation, path: path?.toWellFormed(), contents }, signal).then(result =>
      result.ok || path === undefined ? result : { ok: false as const, error: { ...result.error, path } });
  return {
    exists: (path, signal) => request<boolean>("exists", path, signal),
    list: (path, signal) => request("list", path, signal),
    readText: (path, signal) => request("read-text", path, signal),
    readJson: async (path, signal) => {
      const result = await request<string | undefined>("read-json", path, signal);
      if (!result.ok || result.value === undefined) return result;
      try {
        const value: unknown = JSON.parse(result.value);
        if (isJson(value)) return { ok: true, value };
      } catch {}
      return { ok: false, error: { operation: "read-json", path, reason: "invalid-json" } };
    },
    remove: (path, signal) => request("remove", path, signal),
    writeText: (path, contents, signal) => request("write-text", path, signal, contents.toWellFormed()),
    writeJson: (path, value, signal) => request("write-json", path, signal, `${JSON.stringify(value, null, 2)}\n`),
  };
};
