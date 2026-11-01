import { URL, URLSearchParams, TextDecoder, Headers } from "./web";
type Listener = ((event: Event) => void) | { handleEvent(event: Event): void };
interface Registration {
  readonly listener: Listener;
  readonly capture: boolean;
  readonly once: boolean;
}

export class Event {
  readonly type: string;
  readonly cancelable: boolean;
  target: EventTarget | null = null;
  currentTarget: EventTarget | null = null;
  defaultPrevented = false;
  immediatePropagationStopped = false;

  constructor(type: string, options: { cancelable?: boolean } = {}) {
    this.type = type;
    this.cancelable = options.cancelable === true;
  }

  preventDefault(): void {
    if (this.cancelable) this.defaultPrevented = true;
  }

  stopImmediatePropagation(): void {
    this.immediatePropagationStopped = true;
  }
}

export class EventTarget {
  readonly #listeners = new Map<string, Registration[]>();

  addEventListener(
    type: string,
    listener: Listener | null,
    options: boolean | { once?: boolean; capture?: boolean } = {},
  ): void {
    if (listener === null) return;
    const capture =
      typeof options === "boolean" ? options : options.capture === true;
    const registrations = this.#listeners.get(type) ?? [];
    if (
      registrations.some(
        (entry) => entry.listener === listener && entry.capture === capture,
      )
    )
      return;
    registrations.push({
      listener,
      capture,
      once: typeof options === "object" && options.once === true,
    });
    this.#listeners.set(type, registrations);
  }

  removeEventListener(
    type: string,
    listener: Listener | null,
    options: boolean | { capture?: boolean } = {},
  ): void {
    const capture =
      typeof options === "boolean" ? options : options.capture === true;
    const registrations = this.#listeners.get(type);
    const index =
      registrations?.findIndex(
        (entry) => entry.listener === listener && entry.capture === capture,
      ) ?? -1;
    if (index !== -1) registrations?.splice(index, 1);
  }

  dispatchEvent(event: Event): boolean {
    if (event.currentTarget !== null)
      throw new Error("The event is already being dispatched.");
    event.target = this;
    event.currentTarget = this;
    event.immediatePropagationStopped = false;
    try {
      const registrations = this.#listeners.get(event.type) ?? [];
      for (const entry of [...registrations]) {
        if (!registrations.includes(entry)) continue;
        if (entry.once)
          this.removeEventListener(event.type, entry.listener, entry.capture);
        try {
          if (typeof entry.listener === "function")
            entry.listener.call(this, event);
          else entry.listener.handleEvent(event);
        } catch (error) {
          console.error(error);
        }
        if (event.immediatePropagationStopped) break;
      }
    } finally {
      event.currentTarget = null;
    }
    return !event.defaultPrevented;
  }
}

const defaultAbortReason = (): Error => {
  const error = new Error("This operation was aborted");
  error.name = "AbortError";
  return error;
};

const abort = Symbol("abort");

export class AbortSignal extends EventTarget {
  #aborted = false;
  #reason: unknown;
  #onabort: ((event: Event) => void) | null = null;
  #onabortRegistered = false;
  readonly #dispatchOnabort = (event: Event) =>
    this.#onabort?.call(this, event);

  get aborted(): boolean {
    return this.#aborted;
  }
  get reason(): unknown {
    return this.#reason;
  }
  get onabort(): ((event: Event) => void) | null {
    return this.#onabort;
  }
  set onabort(listener: ((event: Event) => void) | null) {
    if (listener !== null && !this.#onabortRegistered) {
      this.addEventListener("abort", this.#dispatchOnabort);
      this.#onabortRegistered = true;
    }
    this.#onabort = listener;
  }

  throwIfAborted(): void {
    if (this.#aborted) throw this.#reason;
  }

  [abort](reason: unknown): void {
    if (this.#aborted) return;
    this.#aborted = true;
    this.#reason = reason === undefined ? defaultAbortReason() : reason;
    this.dispatchEvent(new Event("abort"));
  }

  static abort(reason?: unknown): AbortSignal {
    const signal = new AbortSignal();
    signal[abort](reason);
    return signal;
  }
}

export class AbortController {
  readonly #signal = new AbortSignal();
  get signal(): AbortSignal {
    return this.#signal;
  }
  abort(reason?: unknown): void {
    this.#signal[abort](reason);
  }
}

export class TextEncoder {
  readonly encoding = "utf-8";

  encode(input = ""): Uint8Array {
    const bytes = unescape(encodeURIComponent(input.toWellFormed()));
    return Uint8Array.from(bytes, (byte) => byte.charCodeAt(0));
  }
}

const format = (value: unknown): string => {
  if (typeof value === "string") return value;
  if (value instanceof Error) return value.stack ?? String(value);
  try {
    const ancestors: object[] = [];
    return (
      JSON.stringify(value, function (this: unknown, _key, entry: unknown) {
        if (entry === null || typeof entry !== "object") return entry;
        while (ancestors.length > 0 && ancestors.at(-1) !== this)
          ancestors.pop();
        if (ancestors.includes(entry)) return "[Circular]";
        ancestors.push(entry);
        return entry;
      }) ?? String(value)
    );
  } catch {
    try {
      return String(value);
    } catch {
      return "[Unserializable]";
    }
  }
};

const host = typeof __host === "undefined" ? undefined : __host;

const log =
  (level: "info" | "warn" | "error" | "debug") =>
  (...values: unknown[]): void =>
    host?.log(level, values.map(format).join(" "));

export const runtimeConsole = Object.freeze({
  log: log("info"),
  info: log("info"),
  warn: log("warn"),
  error: log("error"),
  debug: log("debug"),
});

if (typeof __host !== "undefined") {
  Object.assign(globalThis, {
    URL,
    URLSearchParams,
    TextDecoder,
    Headers,
    Event,
    EventTarget,
    AbortController,
    AbortSignal,
    TextEncoder,
    console: runtimeConsole,
  });
}
