import type {
  AutoZoneConfig,
  OperationAnswer,
  ProjectionEvent,
  ScriptOutcome,
} from "@lucent/game/protocol";

import { completeOperation, startOperation, failOperations, setOperationResponsibility } from "./operations";

import { completeIo } from "./io";

import { prepareJob, failJob, failJobs } from "./jobs";

import type { ScriptSource } from "../game/scripting/ScriptRunner";

declare global {
  var __host: {
    url(input: string, base?: string, key?: string, value?: string): string | undefined;
    form(input: string, serialize: boolean): string;
    decode(bytes: Uint8Array, fatal: boolean, ignoreBOM: boolean): string | undefined;
    swf(method: string, argsJson: string): string | undefined;
    read(queryJson: string): string;
    command(commandJson: string): string;
    acquireConsumableClaim(itemJson: string): string;
    releaseClaim(id: string): void;
    prepareOperation(): string;
    operate(id: string, operationJson: string): void;
    cancelOperation(id: string): void;
    startJob(requestJson: string): string;
    engageJob(id: string, engaged: boolean): void;
    cancelJob(id: string): void;
    requestIo(requestJson: string, body?: Uint8Array): string;
    takeIoBody(id: string): ArrayBuffer;
    cancelIo(id: string): void;
    now(): number;
    log(level: "info" | "warn" | "error" | "debug", text: string): void;
  };
  var __lucent: Readonly<{
    start(
      load: ScriptSource["load"],
      name: string,
      inputsJson: string,
    ): Promise<ScriptOutcome>;
    stop(reason?: string): Promise<void>;
    turn(itemsJson: string): string[];
    wake(): number;
  }>;

  interface Window {
    onHostEventsLost?: () => void;
    onAutoZoneChanged?: (config: AutoZoneConfig) => void;
    onProjectionEvent?: (event: ProjectionEvent) => void;
    onProjectionInvalidated?: () => void;
  }
}

type ScriptEvent =
  | { type: "io"; id: string; result: unknown }
  | { type: "jobprepared"; id: string; label: string; classNames: string[]; warnings: string[] }
  | { type: "jobwarning"; id: string; warning: string }
  | { type: "jobfailed"; id: string; error: string }
  | { type: "operationresponsibility"; id: string; responsible: boolean }
  | { type: "operationstarted" | "operationissued"; id: string }
  | {
      type: "loader";
      name: string;
      args: unknown[];
      projected: ProjectionEvent[];
    }
  | { type: "eventslost" }
  | { type: "autozone"; config: AutoZoneConfig }
  | { type: "invalidated" }
  | {
      type: "operation";
      id: string;
      answer: OperationAnswer | null;
      error: string | null;
    };

interface Timer {
  readonly id: number;
  readonly at: number;
  readonly callback: () => void;
  readonly repeat?: number;
}

const host = __host;

const timers = new Map<number, Timer>();
let nextId = 1;

const schedule = (
  callback: (...args: unknown[]) => void,
  delay: number | undefined,
  args: unknown[],
  repeat: boolean,
): number => {
  const ms = Math.max(0, Number(delay) || 0);
  const timer: Timer = {
    id: nextId++,
    at: host.now() + ms,
    callback: () => callback(...args),
    ...(repeat ? { repeat: ms } : {}),
  };
  timers.set(timer.id, timer);
  return timer.id;
};

const clear = (id: number | undefined): void => {
  if (id !== undefined) timers.delete(id);
};

const describe = (cause: unknown): string => {
  try {
    return cause instanceof Error ? (cause.stack ?? String(cause)) : String(cause);
  } catch {
    return "a value that cannot be converted to text was thrown";
  }
};

export const turn = (itemsJson: string): string[] => {
  const now = host.now();
  const due = [...timers.values()]
    .filter((timer) => timer.at <= now)
    .sort((a, b) => a.at - b.at || a.id - b.id);
  const errors: string[] = [];
  const invoke = (callback: () => void): void => {
    try {
      callback();
    } catch (cause) {
      errors.push(describe(cause));
    }
  };
  const items = JSON.parse(itemsJson) as ScriptEvent[];
  const lost = items.some((item) => item.type === "eventslost");
  for (const item of items) {
    if (item.type === "io") {
      completeIo(item.id, item.result);
    } else if (item.type === "eventslost") {
      const error = new Error("Host events were lost; pending requests failed");
      failOperations(error);
      failJobs(error);
      invoke(() => window.onHostEventsLost?.());
    } else if (item.type === "jobprepared") {
      prepareJob(item.id, item.label, item.warnings, item.classNames);
    } else if (item.type === "jobwarning") {
      host.log("warn", item.warning);
    } else if (item.type === "jobfailed") {
      failJob(item.id, item.error);
    } else if (item.type === "operationresponsibility") {
      setOperationResponsibility(item.id, item.responsible);
    } else if (item.type === "operationstarted") {
      startOperation(item.id);
    } else if (item.type === "operationissued") {
      startOperation(item.id, true);
    } else if (item.type === "loader") {
      invoke(() => {
        const callback: unknown = Reflect.get(window, item.name);
        if (typeof callback === "function") callback.apply(window, item.args);
      });
      for (const event of item.projected) {
        invoke(() => window.onProjectionEvent?.(event));
      }
    } else if (item.type === "autozone") {
      if (!lost) invoke(() => window.onAutoZoneChanged?.(item.config));
    } else if (item.type === "operation") {
      completeOperation(item.id, item.answer, item.error);
    } else if (item.type === "invalidated") {
      invoke(() => window.onProjectionInvalidated?.());
    }
  }
  for (const timer of due) {
    if (timers.get(timer.id) !== timer) continue;
    if (timer.repeat === undefined) {
      timers.delete(timer.id);
    } else {
      timers.set(timer.id, { ...timer, at: host.now() + timer.repeat });
    }
    invoke(timer.callback);
  }
  return errors;
};

export const wake = (): number => {
  let next = Infinity;
  for (const timer of timers.values()) next = Math.min(next, timer.at);
  return next;
};

if (typeof window === "undefined") {
  Object.assign(globalThis, { window: globalThis });
}

Object.assign(globalThis, {
  setTimeout: (callback: (...args: unknown[]) => void, delay?: number, ...args: unknown[]) =>
    schedule(callback, delay, args, false),
  setInterval: (callback: (...args: unknown[]) => void, delay?: number, ...args: unknown[]) =>
    schedule(callback, delay, args, true),
  clearTimeout: clear,
  clearInterval: clear,
});

window.swf = new Proxy({} as Window["swf"], {
  get(_target, method) {
    if (typeof method === "string") {
      return (...args: unknown[]) => {
        const reply = host.swf(method, JSON.stringify(args));
        return reply === undefined ? undefined : JSON.parse(reply);
      };
    }
    return undefined;
  },
});

export const hideInternals = (): void => {
  Reflect.deleteProperty(globalThis, "__host");
  Reflect.deleteProperty(globalThis, "__lucent");
  Reflect.deleteProperty(window, "swf");
};
