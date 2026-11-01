"use strict";
(() => {
  var __defProp = Object.defineProperty;
  var __name = (target, value) => __defProp(target, "name", { value, configurable: true });

  // packages/runtime/src/host/web.ts
  var host = typeof __host === "undefined" ? void 0 : __host;
  var text = /* @__PURE__ */ __name((value) => `${value}`.toWellFormed(), "text");
  var requireArguments = /* @__PURE__ */ __name((actual, required) => {
    if (actual < required) throw new TypeError("Not enough arguments");
  }, "requireArguments");
  var bindParams;
  var replaceParams;
  var URLSearchParams2 = class _URLSearchParams {
    static {
      __name(this, "URLSearchParams");
    }
    #pairs = [];
    #changed;
    constructor(init = "") {
      if (typeof init === "object" && init !== null) {
        if (Symbol.iterator in init) {
          for (const pair of init) {
            if (typeof pair !== "object" || pair === null)
              throw new TypeError("Expected a name/value pair");
            const entry = [...pair];
            if (entry.length !== 2)
              throw new TypeError("Expected a name/value pair");
            this.#pairs.push([text(entry[0]), text(entry[1])]);
          }
        } else
          this.#pairs = Object.entries(init).map(([k, v]) => [text(k), text(v)]);
      } else
        this.#pairs = JSON.parse(
          host.form(text(init).replace(/^\?/, ""), false)
        );
    }
    get size() {
      return this.#pairs.length;
    }
    append(name, value) {
      requireArguments(arguments.length, 2);
      this.#pairs.push([text(name), text(value)]);
      this.#notify();
    }
    delete(name, value) {
      requireArguments(arguments.length, 1);
      name = text(name);
      if (value !== void 0) value = text(value);
      this.#pairs = this.#pairs.filter(
        ([k, v]) => k !== name || value !== void 0 && v !== value
      );
      this.#notify();
    }
    get(name) {
      requireArguments(arguments.length, 1);
      return this.#pairs.find(([k]) => k === text(name))?.[1] ?? null;
    }
    getAll(name) {
      requireArguments(arguments.length, 1);
      name = text(name);
      return this.#pairs.filter(([k]) => k === name).map(([, v]) => v);
    }
    has(name, value) {
      requireArguments(arguments.length, 1);
      name = text(name);
      if (value !== void 0) value = text(value);
      return this.#pairs.some(
        ([k, v]) => k === name && (value === void 0 || v === value)
      );
    }
    set(name, value) {
      requireArguments(arguments.length, 2);
      name = text(name);
      value = text(value);
      const index = this.#pairs.findIndex(([k]) => k === name);
      if (index < 0) this.#pairs.push([name, value]);
      else {
        this.#pairs[index] = [name, value];
        this.#pairs = this.#pairs.filter(([k], i) => k !== name || i === index);
      }
      this.#notify();
    }
    sort() {
      this.#pairs.sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
      this.#notify();
    }
    *entries() {
      for (let i = 0; i < this.#pairs.length; i++)
        yield [...this.#pairs[i]];
    }
    *keys() {
      for (const [k] of this.entries()) yield k;
    }
    *values() {
      for (const [, v] of this.entries()) yield v;
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    forEach(callback3, thisArg) {
      if (typeof callback3 !== "function")
        throw new TypeError("Callback must be a function");
      for (const [k, v] of this) callback3.call(thisArg, v, k, this);
    }
    toString() {
      return host.form(JSON.stringify(this.#pairs), true);
    }
    #notify() {
      this.#changed?.(this.toString());
    }
    static {
      bindParams = /* @__PURE__ */ __name((params, changed) => {
        params.#changed = changed;
      }, "bindParams");
      replaceParams = /* @__PURE__ */ __name((params, search) => {
        params.#pairs = new _URLSearchParams(search).#pairs;
      }, "replaceParams");
    }
  };
  var URL2 = class _URL {
    static {
      __name(this, "URL");
    }
    #parts;
    #params;
    constructor(input, base) {
      this.#parts = _URL.#parse(
        text(input),
        base === void 0 ? void 0 : text(base)
      );
      this.#params = new URLSearchParams2(this.#parts.search);
      bindParams(this.#params, (search) => {
        this.#parts = _URL.#parse(this.href, void 0, "search", search);
      });
    }
    static #parse(input, base, key, value) {
      const result2 = host.url(input, base, key, value);
      if (result2 === void 0) throw new TypeError("Invalid URL");
      return JSON.parse(result2);
    }
    #set(key, value) {
      this.#parts = _URL.#parse(this.href, void 0, key, text(value));
      replaceParams(this.#params, this.search);
    }
    get searchParams() {
      return this.#params;
    }
    get href() {
      return this.#parts.href;
    }
    set href(value) {
      this.#set("href", value);
    }
    get origin() {
      return this.#parts.origin;
    }
    get protocol() {
      return this.#parts.protocol;
    }
    set protocol(value) {
      this.#set("protocol", value);
    }
    get username() {
      return this.#parts.username;
    }
    set username(value) {
      this.#set("username", value);
    }
    get password() {
      return this.#parts.password;
    }
    set password(value) {
      this.#set("password", value);
    }
    get host() {
      return this.#parts.host;
    }
    set host(value) {
      this.#set("host", value);
    }
    get hostname() {
      return this.#parts.hostname;
    }
    set hostname(value) {
      this.#set("hostname", value);
    }
    get port() {
      return this.#parts.port;
    }
    set port(value) {
      this.#set("port", value);
    }
    get pathname() {
      return this.#parts.pathname;
    }
    set pathname(value) {
      this.#set("pathname", value);
    }
    get search() {
      return this.#parts.search;
    }
    set search(value) {
      this.#set("search", value);
    }
    get hash() {
      return this.#parts.hash;
    }
    set hash(value) {
      this.#set("hash", value);
    }
    toString() {
      return this.href;
    }
    toJSON() {
      return this.href;
    }
    static parse(input, base) {
      requireArguments(arguments.length, 1);
      const value = text(input);
      const relative = base === void 0 ? void 0 : text(base);
      try {
        return new _URL(value, relative);
      } catch {
        return null;
      }
    }
    static canParse(input, base) {
      requireArguments(arguments.length, 1);
      const value = text(input);
      const relative = base === void 0 ? void 0 : text(base);
      return host.url(value, relative) !== void 0;
    }
  };
  var TextDecoder2 = class {
    static {
      __name(this, "TextDecoder");
    }
    #fatal;
    #ignoreBOM;
    get encoding() {
      return "utf-8";
    }
    get fatal() {
      return this.#fatal;
    }
    get ignoreBOM() {
      return this.#ignoreBOM;
    }
    constructor(label = "utf-8", options = {}) {
      if (!["utf-8", "utf8"].includes(
        text(label).replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, "").toLowerCase()
      ))
        throw new RangeError("Unsupported encoding");
      this.#fatal = Boolean(options.fatal);
      this.#ignoreBOM = Boolean(options.ignoreBOM);
    }
    decode(input, options = {}) {
      if (options.stream)
        throw new TypeError("Streaming decoding is unsupported");
      let bytes;
      if (input === void 0) bytes = new Uint8Array();
      else if (ArrayBuffer.isView(input))
        bytes = new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
      else if (input instanceof ArrayBuffer) bytes = new Uint8Array(input);
      else throw new TypeError("Expected an ArrayBuffer or view");
      const decoded = host.decode(bytes, this.#fatal, this.#ignoreBOM);
      if (decoded === void 0)
        throw new TypeError("The encoded data was not valid UTF-8");
      return decoded;
    }
  };
  var headerText = /* @__PURE__ */ __name((value) => {
    const result2 = `${value}`;
    if (/[^\x00-\xff]/.test(result2)) throw new TypeError("Expected a ByteString");
    return result2;
  }, "headerText");
  var headerName = /* @__PURE__ */ __name((name) => {
    name = headerText(name);
    if (!/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(name))
      throw new TypeError("Invalid header name");
    return name.toLowerCase();
  }, "headerName");
  var headerValue = /* @__PURE__ */ __name((value) => {
    value = headerText(value).replace(/^[\t\n\r ]+|[\t\n\r ]+$/g, "");
    if (/[\0\r\n]/.test(value)) throw new TypeError("Invalid header value");
    return value;
  }, "headerValue");
  var Headers = class {
    static {
      __name(this, "Headers");
    }
    #values = /* @__PURE__ */ new Map();
    #sorted;
    constructor(init = []) {
      const entries = Symbol.iterator in init ? init : Object.entries(init);
      for (const pair of entries) {
        if (typeof pair !== "object" || pair === null)
          throw new TypeError("Expected a name/value pair");
        const entry = [...pair];
        if (entry.length !== 2) throw new TypeError("Expected a name/value pair");
        this.append(entry[0], entry[1]);
      }
    }
    append(name, value) {
      requireArguments(arguments.length, 2);
      name = headerName(name);
      value = headerValue(value);
      const values = this.#values.get(name) ?? [];
      values.push(value);
      this.#values.set(name, values);
      this.#sorted = void 0;
    }
    delete(name) {
      requireArguments(arguments.length, 1);
      this.#values.delete(headerName(name));
      this.#sorted = void 0;
    }
    get(name) {
      requireArguments(arguments.length, 1);
      name = headerName(name);
      return this.#values.get(name)?.join(name === "cookie" ? "; " : ", ") ?? null;
    }
    has(name) {
      requireArguments(arguments.length, 1);
      return this.#values.has(headerName(name));
    }
    set(name, value) {
      requireArguments(arguments.length, 2);
      name = headerName(name);
      value = headerValue(value);
      this.#values.set(name, [value]);
      this.#sorted = void 0;
    }
    getSetCookie() {
      return [...this.#values.get("set-cookie") ?? []];
    }
    *entries() {
      let index = 0;
      while (true) {
        const pairs = this.#sorted ??= [...this.#values.keys()].sort().flatMap(
          (key) => key === "set-cookie" ? this.#values.get(key).map((value) => [key, value]) : [
            [
              key,
              this.#values.get(key).join(key === "cookie" ? "; " : ", ")
            ]
          ]
        );
        const pair = pairs[index++];
        if (!pair) return;
        yield [...pair];
      }
    }
    *keys() {
      for (const [k] of this) yield k;
    }
    *values() {
      for (const [, v] of this) yield v;
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    forEach(callback3, thisArg) {
      if (typeof callback3 !== "function")
        throw new TypeError("Callback must be a function");
      for (const [k, v] of this) callback3.call(thisArg, v, k, this);
    }
  };

  // packages/runtime/src/host/globals.ts
  var Event = class {
    static {
      __name(this, "Event");
    }
    type;
    cancelable;
    target = null;
    currentTarget = null;
    defaultPrevented = false;
    immediatePropagationStopped = false;
    constructor(type, options = {}) {
      this.type = type;
      this.cancelable = options.cancelable === true;
    }
    preventDefault() {
      if (this.cancelable) this.defaultPrevented = true;
    }
    stopImmediatePropagation() {
      this.immediatePropagationStopped = true;
    }
  };
  var EventTarget = class {
    static {
      __name(this, "EventTarget");
    }
    #listeners = /* @__PURE__ */ new Map();
    addEventListener(type, listener, options = {}) {
      if (listener === null) return;
      const capture = typeof options === "boolean" ? options : options.capture === true;
      const registrations = this.#listeners.get(type) ?? [];
      if (registrations.some(
        (entry) => entry.listener === listener && entry.capture === capture
      ))
        return;
      registrations.push({
        listener,
        capture,
        once: typeof options === "object" && options.once === true
      });
      this.#listeners.set(type, registrations);
    }
    removeEventListener(type, listener, options = {}) {
      const capture = typeof options === "boolean" ? options : options.capture === true;
      const registrations = this.#listeners.get(type);
      const index = registrations?.findIndex(
        (entry) => entry.listener === listener && entry.capture === capture
      ) ?? -1;
      if (index !== -1) registrations?.splice(index, 1);
    }
    dispatchEvent(event) {
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
  };
  var defaultAbortReason = /* @__PURE__ */ __name(() => {
    const error = new Error("This operation was aborted");
    error.name = "AbortError";
    return error;
  }, "defaultAbortReason");
  var abort = /* @__PURE__ */ Symbol("abort");
  var AbortSignal = class _AbortSignal extends EventTarget {
    static {
      __name(this, "AbortSignal");
    }
    #aborted = false;
    #reason;
    #onabort = null;
    #onabortRegistered = false;
    #dispatchOnabort = /* @__PURE__ */ __name((event) => this.#onabort?.call(this, event), "#dispatchOnabort");
    get aborted() {
      return this.#aborted;
    }
    get reason() {
      return this.#reason;
    }
    get onabort() {
      return this.#onabort;
    }
    set onabort(listener) {
      if (listener !== null && !this.#onabortRegistered) {
        this.addEventListener("abort", this.#dispatchOnabort);
        this.#onabortRegistered = true;
      }
      this.#onabort = listener;
    }
    throwIfAborted() {
      if (this.#aborted) throw this.#reason;
    }
    [abort](reason) {
      if (this.#aborted) return;
      this.#aborted = true;
      this.#reason = reason === void 0 ? defaultAbortReason() : reason;
      this.dispatchEvent(new Event("abort"));
    }
    static abort(reason) {
      const signal = new _AbortSignal();
      signal[abort](reason);
      return signal;
    }
  };
  var AbortController2 = class {
    static {
      __name(this, "AbortController");
    }
    #signal = new AbortSignal();
    get signal() {
      return this.#signal;
    }
    abort(reason) {
      this.#signal[abort](reason);
    }
  };
  var TextEncoder2 = class {
    static {
      __name(this, "TextEncoder");
    }
    encoding = "utf-8";
    encode(input = "") {
      const bytes = unescape(encodeURIComponent(input.toWellFormed()));
      return Uint8Array.from(bytes, (byte) => byte.charCodeAt(0));
    }
  };
  var format = /* @__PURE__ */ __name((value) => {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.stack ?? String(value);
    try {
      const ancestors = [];
      return JSON.stringify(value, function(_key, entry) {
        if (entry === null || typeof entry !== "object") return entry;
        while (ancestors.length > 0 && ancestors.at(-1) !== this)
          ancestors.pop();
        if (ancestors.includes(entry)) return "[Circular]";
        ancestors.push(entry);
        return entry;
      }) ?? String(value);
    } catch {
      try {
        return String(value);
      } catch {
        return "[Unserializable]";
      }
    }
  }, "format");
  var host2 = typeof __host === "undefined" ? void 0 : __host;
  var log = /* @__PURE__ */ __name((level) => (...values) => host2?.log(level, values.map(format).join(" ")), "log");
  var runtimeConsole = Object.freeze({
    log: log("info"),
    info: log("info"),
    warn: log("warn"),
    error: log("error"),
    debug: log("debug")
  });
  if (typeof __host !== "undefined") {
    Object.assign(globalThis, {
      URL: URL2,
      URLSearchParams: URLSearchParams2,
      TextDecoder: TextDecoder2,
      Headers,
      Event,
      EventTarget,
      AbortController: AbortController2,
      AbortSignal,
      TextEncoder: TextEncoder2,
      console: runtimeConsole
    });
  }

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Pipeable.js
  var pipeArguments = /* @__PURE__ */ __name((self, args2) => {
    switch (args2.length) {
      case 0:
        return self;
      case 1:
        return args2[0](self);
      case 2:
        return args2[1](args2[0](self));
      case 3:
        return args2[2](args2[1](args2[0](self)));
      case 4:
        return args2[3](args2[2](args2[1](args2[0](self))));
      case 5:
        return args2[4](args2[3](args2[2](args2[1](args2[0](self)))));
      case 6:
        return args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self))))));
      case 7:
        return args2[6](args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self)))))));
      case 8:
        return args2[7](args2[6](args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self))))))));
      case 9:
        return args2[8](args2[7](args2[6](args2[5](args2[4](args2[3](args2[2](args2[1](args2[0](self)))))))));
      default: {
        let ret = self;
        for (let i = 0, len = args2.length; i < len; i++) {
          ret = args2[i](ret);
        }
        return ret;
      }
    }
  }, "pipeArguments");
  var Prototype = {
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var Class = /* @__PURE__ */ (function() {
    function PipeableBase() {
    }
    __name(PipeableBase, "PipeableBase");
    PipeableBase.prototype = Prototype;
    return PipeableBase;
  })();

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Function.js
  var dual = /* @__PURE__ */ __name(function(arity, body) {
    if (typeof arity === "function") {
      return function() {
        return arity(arguments) ? body.apply(this, arguments) : (self) => body(self, ...arguments);
      };
    }
    switch (arity) {
      case 0:
      case 1:
        throw new RangeError(`Invalid arity ${arity}`);
      case 2:
        return function(a, b) {
          if (arguments.length >= 2) {
            return body(a, b);
          }
          return function(self) {
            return body(self, a);
          };
        };
      case 3:
        return function(a, b, c) {
          if (arguments.length >= 3) {
            return body(a, b, c);
          }
          return function(self) {
            return body(self, a, b);
          };
        };
      default:
        return function() {
          if (arguments.length >= arity) {
            return body.apply(this, arguments);
          }
          const args2 = arguments;
          return function(self) {
            return body(self, ...args2);
          };
        };
    }
  }, "dual");
  var identity = /* @__PURE__ */ __name((a) => a, "identity");
  var constant = /* @__PURE__ */ __name((value) => () => value, "constant");
  var constTrue = /* @__PURE__ */ constant(true);
  var constFalse = /* @__PURE__ */ constant(false);
  var constNull = /* @__PURE__ */ constant(null);
  var constUndefined = /* @__PURE__ */ constant(void 0);
  var constVoid = constUndefined;
  function pipe(a, ...args2) {
    return pipeArguments(a, args2);
  }
  __name(pipe, "pipe");
  function memoize(f) {
    const cache2 = /* @__PURE__ */ new WeakMap();
    return (a) => {
      const cached2 = cache2.get(a);
      if (cached2 !== void 0) return cached2;
      const result2 = f(a);
      cache2.set(a, result2);
      return result2;
    };
  }
  __name(memoize, "memoize");
  function memoizeIdempotent(f) {
    const cache2 = /* @__PURE__ */ new WeakMap();
    return (a) => {
      const cached2 = cache2.get(a);
      if (cached2 !== void 0) return cached2;
      const result2 = f(a);
      cache2.set(a, result2);
      if (result2 !== a) {
        cache2.set(result2, result2);
      }
      return result2;
    };
  }
  __name(memoizeIdempotent, "memoizeIdempotent");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/equal.js
  var getAllObjectKeys = /* @__PURE__ */ __name((obj) => {
    const keys = new Set(Reflect.ownKeys(obj));
    if (obj.constructor === Object) return keys;
    if (obj instanceof Error) {
      keys.delete("stack");
    }
    const proto = Object.getPrototypeOf(obj);
    let current = proto;
    while (current !== null && current !== Object.prototype) {
      const ownKeys = Reflect.ownKeys(current);
      for (let i = 0; i < ownKeys.length; i++) {
        keys.add(ownKeys[i]);
      }
      current = Object.getPrototypeOf(current);
    }
    if (keys.has("constructor") && typeof obj.constructor === "function" && proto === obj.constructor.prototype) {
      keys.delete("constructor");
    }
    return keys;
  }, "getAllObjectKeys");
  var byReferenceInstances = /* @__PURE__ */ new WeakSet();
  var viewBytes = /* @__PURE__ */ __name((view) => new Uint8Array(view.buffer, view.byteOffset, view.byteLength), "viewBytes");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/hash.js
  var backEdges = 0;
  var addBackEdge = /* @__PURE__ */ __name(() => {
    backEdges++;
  }, "addBackEdge");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Predicate.js
  function isString(input) {
    return typeof input === "string";
  }
  __name(isString, "isString");
  function isNumber(input) {
    return typeof input === "number";
  }
  __name(isNumber, "isNumber");
  function isBoolean(input) {
    return typeof input === "boolean";
  }
  __name(isBoolean, "isBoolean");
  function isSymbol(input) {
    return typeof input === "symbol";
  }
  __name(isSymbol, "isSymbol");
  function isPropertyKey(u) {
    return isString(u) || isNumber(u) || isSymbol(u);
  }
  __name(isPropertyKey, "isPropertyKey");
  function isFunction(input) {
    return typeof input === "function";
  }
  __name(isFunction, "isFunction");
  function isNotNullish(input) {
    return input != null;
  }
  __name(isNotNullish, "isNotNullish");
  function isNever(_) {
    return false;
  }
  __name(isNever, "isNever");
  function isUnknown(_) {
    return true;
  }
  __name(isUnknown, "isUnknown");
  function isObject(input) {
    return typeof input === "object" && input !== null && !Array.isArray(input);
  }
  __name(isObject, "isObject");
  function isObjectKeyword(input) {
    return typeof input === "object" && input !== null || isFunction(input);
  }
  __name(isObjectKeyword, "isObjectKeyword");
  var hasProperty = /* @__PURE__ */ dual(2, (self, property) => isObjectKeyword(self) && property in self);
  var isTagged = /* @__PURE__ */ dual(2, (self, tag2) => hasProperty(self, "_tag") && self["_tag"] === tag2);
  function isError(input) {
    return input instanceof Error;
  }
  __name(isError, "isError");
  function isIterable(input) {
    return hasProperty(input, Symbol.iterator) || isString(input);
  }
  __name(isIterable, "isIterable");
  function isPromiseLike(input) {
    return hasProperty(input, "then") && isFunction(input.then);
  }
  __name(isPromiseLike, "isPromiseLike");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Hash.js
  var symbol = "~effect/Hash";
  var hash = /* @__PURE__ */ __name((self) => {
    switch (typeof self) {
      case "number":
        return number(self);
      case "bigint":
        return string(self.toString(10));
      case "string":
        return string(self);
      case "function":
      case "object": {
        if (self === null) {
          break;
        } else if (self instanceof Date) {
          if (Number.isNaN(self.getTime())) {
            return string("Invalid Date");
          }
          return string(self.toISOString());
        } else if (self instanceof RegExp) {
          return string(self.toString());
        } else {
          if (byReferenceInstances.has(self)) {
            return random(self);
          }
          const cached2 = hashCache.get(self);
          if (cached2 !== void 0) {
            return cached2;
          }
          if (visitedObjects.has(self)) {
            addBackEdge();
            return string("[Circular]");
          }
          visitedObjects.add(self);
          const seen = backEdges;
          let h;
          try {
            if (symbol in self) {
              h = self[symbol]();
            } else if (typeof self === "function") {
              h = random(self);
            } else if (self instanceof DataView) {
              h = array(viewBytes(self));
            } else if (Array.isArray(self) || ArrayBuffer.isView(self)) {
              h = array(self);
            } else if (self instanceof Map) {
              h = hashMap(self);
            } else if (self instanceof Set) {
              h = hashSet(self);
            } else {
              h = structure(self);
            }
          } finally {
            visitedObjects.delete(self);
          }
          if (seen === backEdges) {
            hashCache.set(self, h);
          }
          return h;
        }
      }
    }
    return optimize(mix(string(String(self))));
  }, "hash");
  var random = /* @__PURE__ */ __name((self) => {
    if (!randomHashCache.has(self)) {
      randomHashCache.set(self, optimize(Math.random() * 4294967296 | 0));
    }
    return randomHashCache.get(self);
  }, "random");
  var mix = /* @__PURE__ */ __name((h) => {
    h ^= h >>> 16;
    h = Math.imul(h, 2246822507);
    h ^= h >>> 13;
    h = Math.imul(h, 3266489909);
    return h ^ h >>> 16;
  }, "mix");
  var combine = /* @__PURE__ */ dual(2, (self, b) => mix(Math.imul(self, 2654435761) + Math.imul(b, 2246822507)));
  var optimize = /* @__PURE__ */ __name((n) => n & 3221225471 | n >>> 1 & 1073741824, "optimize");
  var float64 = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(8));
  var number = /* @__PURE__ */ __name((n) => {
    const h = n | 0;
    if (h === n) {
      return optimize(h);
    }
    float64.setFloat64(0, n !== n ? NaN : n);
    return optimize(combine(float64.getInt32(0), float64.getInt32(4)));
  }, "number");
  var string = /* @__PURE__ */ __name((str) => {
    let h = 5381, i = str.length;
    while (i) {
      h = h * 33 ^ str.charCodeAt(--i);
    }
    return optimize(h);
  }, "string");
  var structureKeys = /* @__PURE__ */ __name((o, keys) => {
    let h = 12289;
    for (const key of keys) {
      h ^= combine(hash(key), hash(o[key]));
    }
    return optimize(h);
  }, "structureKeys");
  var structure = /* @__PURE__ */ __name((o) => structureKeys(o, getAllObjectKeys(o)), "structure");
  var unordered = /* @__PURE__ */ __name((seed, f) => (iter) => {
    let h = seed;
    for (const element of iter) {
      h ^= f(element);
    }
    return optimize(h);
  }, "unordered");
  var array = /* @__PURE__ */ __name((arr) => {
    let h = 6151;
    for (const element of arr) {
      h = combine(h, hash(element));
    }
    return optimize(h);
  }, "array");
  var hashMap = /* @__PURE__ */ unordered(/* @__PURE__ */ string("Map"), ([k, v]) => combine(hash(k), hash(v)));
  var setSeed = /* @__PURE__ */ string("Set");
  var hashSet = /* @__PURE__ */ unordered(setSeed, (element) => combine(setSeed, hash(element)));
  var randomHashCache = /* @__PURE__ */ new WeakMap();
  var hashCache = /* @__PURE__ */ new WeakMap();
  var visitedObjects = /* @__PURE__ */ new WeakSet();

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Equal.js
  var symbol2 = "~effect/Equal";
  function equals() {
    if (arguments.length === 1) {
      return (self) => compareBoth(self, arguments[0]);
    }
    return compareBoth(arguments[0], arguments[1]);
  }
  __name(equals, "equals");
  function compareBoth(self, that) {
    if (self === that) return true;
    if (self == null || that == null) return false;
    const selfType = typeof self;
    if (selfType !== typeof that) {
      return false;
    }
    if (selfType === "number" && self !== self && that !== that) {
      return true;
    }
    if (selfType !== "object" && selfType !== "function") {
      return false;
    }
    if (byReferenceInstances.has(self) || byReferenceInstances.has(that)) {
      return false;
    }
    return compareObjects(self, that);
  }
  __name(compareBoth, "compareBoth");
  function compareObjects(self, that) {
    const depth = pathLeft.length;
    for (let i = depth; i-- > 0; ) {
      if (pathLeft[i] === self && pathRight[i] === that) return true;
    }
    if (depth) return compareOnPath(self, that);
    let known = results.get(self);
    if (!known) results.set(self, known = /* @__PURE__ */ new WeakMap());
    let result2 = known.get(that);
    if (result2 === void 0) known.set(that, result2 = compareOnPath(self, that));
    return result2;
  }
  __name(compareObjects, "compareObjects");
  function compareOnPath(self, that) {
    pathLeft.push(self);
    pathRight.push(that);
    try {
      return compareStructure(self, that);
    } finally {
      pathLeft.pop();
      pathRight.pop();
    }
  }
  __name(compareOnPath, "compareOnPath");
  var pathLeft = [];
  var pathRight = [];
  var results = /* @__PURE__ */ new WeakMap();
  function compareStructure(self, that) {
    if (hash(self) !== hash(that)) {
      return false;
    } else if (self instanceof Date) {
      if (!(that instanceof Date)) return false;
      const selfTime = self.getTime();
      const thatTime = that.getTime();
      return selfTime === thatTime || Number.isNaN(selfTime) && Number.isNaN(thatTime);
    } else if (self instanceof RegExp) {
      return that instanceof RegExp && self.toString() === that.toString();
    }
    const bothEquals = isEqual(self);
    if (bothEquals !== isEqual(that) || typeof self === "function" && !bothEquals) {
      return false;
    } else if (bothEquals) {
      return self[symbol2](that);
    } else if (Array.isArray(self)) {
      if (!Array.isArray(that) || self.length !== that.length) {
        return false;
      }
      return compareArrays(self, that);
    } else if (ArrayBuffer.isView(self)) {
      const selfIsDataView = self instanceof DataView;
      if (!ArrayBuffer.isView(that) || self.byteLength !== that.byteLength || selfIsDataView !== that instanceof DataView) {
        return false;
      }
      if (selfIsDataView) {
        return compareTypedArrays(viewBytes(self), viewBytes(that));
      }
      return compareTypedArrays(self, that);
    } else if (self instanceof Map) {
      if (!(that instanceof Map) || self.size !== that.size) {
        return false;
      }
      return compareHashed(self, that, entryHash, equalEntries);
    } else if (self instanceof Set) {
      if (!(that instanceof Set) || self.size !== that.size) {
        return false;
      }
      return compareHashed(self, that, hash, compareBoth);
    }
    return compareRecords(self, that);
  }
  __name(compareStructure, "compareStructure");
  function compareArrays(self, that) {
    for (let i = 0; i < self.length; i++) {
      if (!compareBoth(self[i], that[i])) {
        return false;
      }
    }
    return true;
  }
  __name(compareArrays, "compareArrays");
  function compareTypedArrays(self, that) {
    if (self.length !== that.length) {
      return false;
    }
    for (let i = 0; i < self.length; i++) {
      if (self[i] !== that[i]) {
        return false;
      }
    }
    return true;
  }
  __name(compareTypedArrays, "compareTypedArrays");
  function compareRecords(self, that) {
    const selfKeys = getAllObjectKeys(self);
    const thatKeys = getAllObjectKeys(that);
    if (selfKeys.size !== thatKeys.size) {
      return false;
    }
    for (const key of selfKeys) {
      if (!thatKeys.has(key) || !compareBoth(self[key], that[key])) {
        return false;
      }
    }
    return true;
  }
  __name(compareRecords, "compareRecords");
  function compareHashed(self, that, hashOf, equivalent) {
    const groups = /* @__PURE__ */ new Map();
    let remaining = 0;
    for (const item of that) {
      const h = hashOf(item);
      const group = groups.get(h);
      if (group) group.push(item);
      else groups.set(h, [item]);
      remaining++;
    }
    outer: for (const item of self) {
      const group = groups.get(hashOf(item));
      if (group) {
        for (let i = 0; i < group.length; i++) {
          if (equivalent(item, group[i])) {
            group[i] = group[group.length - 1];
            group.pop();
            remaining--;
            continue outer;
          }
        }
      }
      return false;
    }
    return remaining === 0;
  }
  __name(compareHashed, "compareHashed");
  var entryHash = /* @__PURE__ */ __name((entry) => hash(entry[0]), "entryHash");
  var equalEntries = /* @__PURE__ */ __name((self, that) => compareBoth(self[0], that[0]) && compareBoth(self[1], that[1]), "equalEntries");
  var isEqual = /* @__PURE__ */ __name((u) => hasProperty(u, symbol2), "isEqual");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Redactable.js
  var symbolRedactable = /* @__PURE__ */ Symbol.for("~effect/Redactable");
  var isRedactable = /* @__PURE__ */ __name((u) => hasProperty(u, symbolRedactable), "isRedactable");
  function redact(u) {
    if (isRedactable(u)) return getRedacted(u);
    return u;
  }
  __name(redact, "redact");
  function getRedacted(redactable) {
    return redactable[symbolRedactable](globalThis[currentFiberTypeId]?.context ?? emptyContext);
  }
  __name(getRedacted, "getRedacted");
  var currentFiberTypeId = "~effect/Fiber/currentFiber";
  var emptyMap = /* @__PURE__ */ new Map();
  var emptyContext = {
    "~effect/Context": {},
    base: emptyMap,
    depth: 0,
    mapUnsafe: emptyMap,
    pipe() {
      return pipeArguments(this, arguments);
    }
  };

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Formatter.js
  function format2(input, options) {
    const space = options?.space ?? 0;
    const ancestors = /* @__PURE__ */ new WeakSet();
    const gap = !space ? "" : typeof space === "number" ? " ".repeat(space) : space;
    const ind = /* @__PURE__ */ __name((d) => gap.repeat(d), "ind");
    const wrap = /* @__PURE__ */ __name((v, body) => {
      const ctor = v?.constructor;
      return ctor && ctor !== Object.prototype.constructor && ctor.name ? `${ctor.name}(${body})` : body;
    }, "wrap");
    const ownKeys = /* @__PURE__ */ __name((o) => {
      try {
        return Reflect.ownKeys(o);
      } catch {
        return ["[ownKeys threw]"];
      }
    }, "ownKeys");
    function recur(v, d = 0) {
      try {
        return recurUnsafe(v, d);
      } catch {
        if (typeof v === "object" && v !== null || typeof v === "function") ancestors.delete(v);
        return "[inspection threw]";
      }
    }
    __name(recur, "recur");
    function recurUnsafe(v, d = 0) {
      if (typeof v === "string") return JSON.stringify(v);
      if (typeof v === "number" || v == null || typeof v === "boolean" || typeof v === "symbol") return String(v);
      if (typeof v === "bigint") return String(v) + "n";
      if (typeof v === "object" || typeof v === "function") {
        if (ancestors.has(v)) return CIRCULAR;
        ancestors.add(v);
        let output;
        if (symbolRedactable in v) {
          output = recur(getRedacted(v), d);
        } else if (Array.isArray(v)) {
          output = !gap || v.length <= 1 ? `[${v.map((x) => recur(x, d)).join(",")}]` : `[
${ind(d + 1)}${v.map((x) => recur(x, d + 1)).join(",\n" + ind(d + 1))}
${ind(d)}]`;
        } else if (v instanceof Date) {
          output = formatDate(v);
        } else if (!options?.ignoreToString && hasProperty(v, "toString") && typeof v["toString"] === "function" && v["toString"] !== Object.prototype.toString && v["toString"] !== Array.prototype.toString) {
          const s = safeToString(v);
          output = v instanceof Error && v.cause !== void 0 ? `${s} (cause: ${recur(v.cause, d)})` : s;
        } else if (Symbol.iterator in v) {
          output = `${v.constructor.name}(${recur(Array.from(v), d)})`;
        } else {
          const keys = ownKeys(v);
          if (!gap || keys.length <= 1) {
            const body = `{${keys.map((k) => `${formatPropertyKey(k)}:${recur(safeGet(v, k), d)}`).join(",")}}`;
            output = wrap(v, body);
          } else {
            const body = `{
${keys.map((k) => `${ind(d + 1)}${formatPropertyKey(k)}: ${recur(safeGet(v, k), d + 1)}`).join(",\n")}
${ind(d)}}`;
            output = wrap(v, body);
          }
        }
        ancestors.delete(v);
        return output;
      }
      return String(v);
    }
    __name(recurUnsafe, "recurUnsafe");
    return recur(input, 0);
  }
  __name(format2, "format");
  var CIRCULAR = "[Circular]";
  function formatPropertyKey(name) {
    return typeof name === "string" ? JSON.stringify(name) : String(name);
  }
  __name(formatPropertyKey, "formatPropertyKey");
  function formatPath(path) {
    return path.map((key) => `[${formatPropertyKey(key)}]`).join("");
  }
  __name(formatPath, "formatPath");
  function formatDate(date2) {
    try {
      return date2.toISOString();
    } catch {
      return "Invalid Date";
    }
  }
  __name(formatDate, "formatDate");
  function safeToString(input) {
    try {
      const s = input.toString();
      return typeof s === "string" ? s : String(s);
    } catch {
      return "[toString threw]";
    }
  }
  __name(safeToString, "safeToString");
  function safeGet(input, key) {
    try {
      return input[key];
    } catch {
      return "[property access threw]";
    }
  }
  __name(safeGet, "safeGet");
  function formatJson(input, options) {
    const ancestors = [];
    return JSON.stringify(input, function(key, value) {
      const original = Object.getOwnPropertyDescriptor(this, key)?.value;
      const redacted = hasProperty(original, symbolRedactable) ? redact(original) : redact(value);
      if (typeof redacted === "bigint") {
        return format2(redacted);
      }
      if (typeof redacted !== "object" || redacted === null) {
        return redacted;
      }
      const current = redacted instanceof Error && !hasProperty(redacted, "toJSON") ? {
        ...redacted,
        name: redacted.name,
        message: redacted.message
      } : redacted;
      while (ancestors.length > 0 && ancestors[ancestors.length - 1] !== this) {
        ancestors.pop();
      }
      if (ancestors.includes(redacted)) {
        return void 0;
      }
      ancestors.push(redacted);
      if (current !== redacted) {
        ancestors.push(current);
      }
      if (!hasGetter(current) || isJsonPrimitiveWrapper(current)) {
        return current;
      }
      const serialized = new Proxy(current, {
        get(target, key2) {
          const value2 = Reflect.get(target, key2, target);
          return Object.getOwnPropertyDescriptor(target, key2)?.get !== void 0 ? redact(value2) : value2;
        }
      });
      ancestors.push(serialized);
      return serialized;
    }, options?.space) ?? "null";
  }
  __name(formatJson, "formatJson");
  function hasGetter(object) {
    for (const key of Object.getOwnPropertyNames(object)) {
      if (Object.getOwnPropertyDescriptor(object, key)?.get !== void 0) {
        return true;
      }
    }
    return false;
  }
  __name(hasGetter, "hasGetter");
  function isJsonPrimitiveWrapper(object) {
    for (const valueOf of [Number.prototype.valueOf, Boolean.prototype.valueOf, String.prototype.valueOf]) {
      try {
        Reflect.apply(valueOf, object, []);
        return true;
      } catch {
      }
    }
    return false;
  }
  __name(isJsonPrimitiveWrapper, "isJsonPrimitiveWrapper");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Inspectable.js
  var NodeInspectSymbol = /* @__PURE__ */ Symbol.for("nodejs.util.inspect.custom");
  var toJson = /* @__PURE__ */ __name((input) => {
    try {
      input = redact(input);
      if (hasProperty(input, "toJSON") && isFunction(input["toJSON"]) && input["toJSON"].length === 0) {
        return input.toJSON();
      } else if (Array.isArray(input)) {
        return input.map(toJson);
      }
      return input;
    } catch {
      return "[toJSON threw]";
    }
  }, "toJson");
  var toStringUnknown = /* @__PURE__ */ __name((u, whitespace = 2) => {
    if (typeof u === "string") {
      return u;
    }
    try {
      return typeof u === "object" ? formatJson(u, {
        space: whitespace
      }) : format2(u, {
        space: whitespace
      });
    } catch {
      return String(u);
    }
  }, "toStringUnknown");
  var BaseProto = {
    toJSON() {
      return toJson(this);
    },
    [NodeInspectSymbol]() {
      return this.toJSON();
    },
    toString() {
      return format2(this.toJSON());
    }
  };
  var Class2 = class {
    static {
      __name(this, "Class");
    }
    /**
     * Node.js custom inspection method.
     *
     * **When to use**
     *
     * Use to expose the class JSON representation to Node.js inspection.
     *
     * @since 2.0.0
     */
    [NodeInspectSymbol]() {
      return this.toJSON();
    }
    /**
     * Returns a formatted string representation of this object.
     *
     * **When to use**
     *
     * Use to format the class JSON representation as a string.
     *
     * @since 2.0.0
     */
    toString() {
      return format2(this.toJSON());
    }
  };

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/stackTraceLimit.js
  var isStackTraceLimitWritable = /* @__PURE__ */ __name(() => {
    const desc = Object.getOwnPropertyDescriptor(Error, "stackTraceLimit");
    if (desc === void 0) {
      return Object.isExtensible(Error);
    }
    return Object.hasOwn(desc, "writable") ? desc.writable === true : desc.set !== void 0;
  }, "isStackTraceLimitWritable");
  var canWriteStackTraceLimit = /* @__PURE__ */ isStackTraceLimitWritable();
  var getStackTraceLimit = /* @__PURE__ */ __name(() => Error.stackTraceLimit, "getStackTraceLimit");
  var setStackTraceLimit = /* @__PURE__ */ __name((value) => {
    if (canWriteStackTraceLimit) {
      ;
      Error.stackTraceLimit = value;
    }
  }, "setStackTraceLimit");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Utils.js
  var SingleShotGen = class {
    static {
      __name(this, "SingleShotGen");
    }
    constructor(self) {
      this.value = self;
      this.done = false;
    }
    /**
     * Yields the stored value once, then completes with the value sent back in.
     *
     * **When to use**
     *
     * Use to advance a `SingleShotGen` through its single yield and completion
     * step.
     *
     * @since 2.0.0
     */
    next(a) {
      if (this.done) {
        this.value = a;
        return this;
      }
      this.done = true;
      return {
        value: this.value,
        done: false
      };
    }
  };
  var pickInternalCall = /* @__PURE__ */ __name(() => {
    const InternalTypeId = "~effect/Utils/internal";
    const standard = {
      [InternalTypeId]: (body) => {
        return body();
      }
    };
    const forced = {
      [InternalTypeId]: (body) => {
        try {
          return body();
        } finally {
        }
      }
    };
    const isNotOptimizedAway = getStackTraceLimit() !== 0 && standard[InternalTypeId](() => new Error().stack)?.includes(InternalTypeId) === true;
    return isNotOptimizedAway ? standard[InternalTypeId] : forced[InternalTypeId];
  }, "pickInternalCall");
  var internalCall = /* @__PURE__ */ pickInternalCall();

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/record.js
  function assignProperty(self, key, value) {
    if (key === "__proto__") {
      Object.defineProperty(self, key, {
        value,
        writable: true,
        enumerable: true,
        configurable: true
      });
    } else {
      ;
      self[key] = value;
    }
  }
  __name(assignProperty, "assignProperty");
  function assignProperties(self, source) {
    for (const key of Reflect.ownKeys(source)) {
      if (Object.prototype.propertyIsEnumerable.call(source, key)) {
        assignProperty(self, key, source[key]);
      }
    }
  }
  __name(assignProperties, "assignProperties");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/core.js
  var EffectTypeId = `~effect/Effect`;
  var ExitTypeId = `~effect/Exit`;
  var effectVariance = {
    _A: identity,
    _E: identity,
    _R: identity
  };
  var identifier = `${EffectTypeId}/identifier`;
  var args = `${EffectTypeId}/args`;
  var evaluate = `${EffectTypeId}/evaluate`;
  var contA = `${EffectTypeId}/successCont`;
  var contE = `${EffectTypeId}/failureCont`;
  var contAll = `${EffectTypeId}/ensureCont`;
  var Yield = /* @__PURE__ */ Symbol.for("effect/Effect/Yield");
  var PipeInspectableProto = {
    pipe() {
      return pipeArguments(this, arguments);
    },
    toJSON() {
      return {
        ...this
      };
    },
    toString() {
      return format2(this.toJSON(), {
        ignoreToString: true,
        space: 2
      });
    },
    [NodeInspectSymbol]() {
      return this.toJSON();
    }
  };
  var StructuralProto = {
    [symbol]() {
      return structureKeys(this, Object.keys(this));
    },
    [symbol2](that) {
      const selfKeys = Object.keys(this);
      const thatKeys = Object.keys(that);
      if (selfKeys.length !== thatKeys.length) return false;
      for (let i = 0; i < selfKeys.length; i++) {
        if (selfKeys[i] !== thatKeys[i] || !equals(this[selfKeys[i]], that[selfKeys[i]])) {
          return false;
        }
      }
      return true;
    }
  };
  var EffectProto = {
    [EffectTypeId]: effectVariance,
    ...PipeInspectableProto,
    [Symbol.iterator]() {
      return new SingleShotGen(this);
    },
    toJSON() {
      return {
        _id: "Effect",
        op: this[identifier],
        ...args in this ? {
          args: this[args]
        } : void 0
      };
    }
  };
  var isEffect = /* @__PURE__ */ __name((u) => hasProperty(u, EffectTypeId), "isEffect");
  var isExit = /* @__PURE__ */ __name((u) => hasProperty(u, ExitTypeId), "isExit");
  var CauseTypeId = "~effect/Cause";
  var CauseReasonTypeId = "~effect/Cause/Reason";
  var isCause = /* @__PURE__ */ __name((self) => hasProperty(self, CauseTypeId), "isCause");
  var CauseImpl = class {
    static {
      __name(this, "CauseImpl");
    }
    constructor(failures) {
      this[CauseTypeId] = CauseTypeId;
      this.reasons = failures;
    }
    pipe() {
      return pipeArguments(this, arguments);
    }
    toJSON() {
      return {
        _id: "Cause",
        failures: this.reasons.map((f) => f.toJSON())
      };
    }
    toString() {
      return `Cause(${format2(this.reasons)})`;
    }
    [NodeInspectSymbol]() {
      return this.toJSON();
    }
    [symbol2](that) {
      return isCause(that) && this.reasons.length === that.reasons.length && this.reasons.every((e, i) => equals(e, that.reasons[i]));
    }
    [symbol]() {
      return array(this.reasons);
    }
  };
  var annotationsMap = /* @__PURE__ */ new WeakMap();
  var ReasonBase = class {
    static {
      __name(this, "ReasonBase");
    }
    [CauseReasonTypeId];
    annotations;
    _tag;
    constructor(_tag, annotations, originalError) {
      this[CauseReasonTypeId] = CauseReasonTypeId;
      this._tag = _tag;
      if (annotations !== constEmptyAnnotations && typeof originalError === "object" && originalError !== null && annotations.size > 0) {
        const prevAnnotations = annotationsMap.get(originalError);
        if (prevAnnotations) {
          annotations = new Map([...prevAnnotations, ...annotations]);
        }
        annotationsMap.set(originalError, annotations);
      }
      this.annotations = annotations;
    }
    annotate(annotations, options) {
      if (annotations.mapUnsafe.size === 0) return this;
      const newAnnotations = new Map(this.annotations);
      annotations.mapUnsafe.forEach((value, key) => {
        if (options?.overwrite !== true && newAnnotations.has(key)) return;
        newAnnotations.set(key, value);
      });
      const self = Object.assign(Object.create(Object.getPrototypeOf(this)), this);
      self.annotations = newAnnotations;
      return self;
    }
    pipe() {
      return pipeArguments(this, arguments);
    }
    toString() {
      return format2(this);
    }
    [NodeInspectSymbol]() {
      return this.toString();
    }
  };
  var constEmptyAnnotations = /* @__PURE__ */ new Map();
  var Fail = class extends ReasonBase {
    static {
      __name(this, "Fail");
    }
    constructor(error, annotations = constEmptyAnnotations) {
      super("Fail", annotations, error);
      this.error = error;
    }
    toString() {
      return `Fail(${format2(this.error)})`;
    }
    toJSON() {
      return {
        _tag: "Fail",
        error: this.error
      };
    }
    [symbol2](that) {
      return isFailReason(that) && equals(this.error, that.error) && equals(this.annotations, that.annotations);
    }
    [symbol]() {
      return combine(string(this._tag))(combine(hash(this.error))(hash(this.annotations)));
    }
  };
  var causeFromReasons = /* @__PURE__ */ __name((reasons) => new CauseImpl(reasons), "causeFromReasons");
  var dedupeReasons = /* @__PURE__ */ __name((self, that) => {
    const buckets = /* @__PURE__ */ new Map();
    const out = [];
    for (const reason of self.concat(that)) {
      const hash2 = hash(reason);
      const bucket = buckets.get(hash2);
      if (bucket === void 0) {
        buckets.set(hash2, [reason]);
      } else if (bucket.some((previous) => equals(previous, reason))) {
        continue;
      } else {
        bucket.push(reason);
      }
      out.push(reason);
    }
    return out;
  }, "dedupeReasons");
  var causeCombine = /* @__PURE__ */ dual(2, (self, that) => {
    if (self.reasons.length === 0) {
      return that;
    } else if (that.reasons.length === 0) {
      return self;
    }
    const newCause = new CauseImpl(dedupeReasons(self.reasons, that.reasons));
    return equals(self, newCause) ? self : newCause;
  });
  var causeEmpty = /* @__PURE__ */ new CauseImpl([]);
  var causeFail = /* @__PURE__ */ __name((error) => new CauseImpl([new Fail(error)]), "causeFail");
  var Die = class extends ReasonBase {
    static {
      __name(this, "Die");
    }
    constructor(defect2, annotations = constEmptyAnnotations) {
      super("Die", annotations, defect2);
      this.defect = defect2;
    }
    toString() {
      return `Die(${format2(this.defect)})`;
    }
    toJSON() {
      return {
        _tag: "Die",
        defect: this.defect
      };
    }
    [symbol2](that) {
      return isDieReason(that) && equals(this.defect, that.defect) && equals(this.annotations, that.annotations);
    }
    [symbol]() {
      return combine(string(this._tag))(combine(hash(this.defect))(hash(this.annotations)));
    }
  };
  var causeDie = /* @__PURE__ */ __name((defect2) => new CauseImpl([new Die(defect2)]), "causeDie");
  var causeAnnotate = /* @__PURE__ */ dual((args2) => isCause(args2[0]), (self, annotations, options) => {
    if (annotations.mapUnsafe.size === 0) return self;
    return new CauseImpl(self.reasons.map((f) => f.annotate(annotations, options)));
  });
  var isFailReason = /* @__PURE__ */ __name((self) => self._tag === "Fail", "isFailReason");
  var isDieReason = /* @__PURE__ */ __name((self) => self._tag === "Die", "isDieReason");
  var isInterruptReason = /* @__PURE__ */ __name((self) => self._tag === "Interrupt", "isInterruptReason");
  function defaultEvaluate(_fiber) {
    return exitDie(`Effect.evaluate: Not implemented`);
  }
  __name(defaultEvaluate, "defaultEvaluate");
  var makePrimitiveProto = /* @__PURE__ */ __name((options) => ({
    ...EffectProto,
    [identifier]: options.op,
    [evaluate]: options[evaluate] ?? defaultEvaluate,
    [contA]: options[contA],
    [contE]: options[contE],
    [contAll]: options[contAll]
  }), "makePrimitiveProto");
  var makePrimitive = /* @__PURE__ */ __name((options) => {
    const Proto3 = makePrimitiveProto(options);
    const PrimitiveImpl = /* @__PURE__ */ __name(function(value) {
      this[args] = value;
    }, "PrimitiveImpl");
    PrimitiveImpl.prototype = Proto3;
    return function(value) {
      return new PrimitiveImpl(value);
    };
  }, "makePrimitive");
  var makeExit = /* @__PURE__ */ __name((options) => {
    const Proto3 = {
      [ExitTypeId]: ExitTypeId,
      _tag: options.op,
      get [options.prop]() {
        return this[args];
      },
      ...makePrimitiveProto(options),
      toString() {
        return `${options.op}(${format2(this[args])})`;
      },
      toJSON() {
        return {
          _id: "Exit",
          _tag: options.op,
          [options.prop]: this[args]
        };
      },
      [symbol2](that) {
        return isExit(that) && that._tag === this._tag && equals(this[args], that[args]);
      },
      [symbol]() {
        return combine(string(options.op), hash(this[args]));
      }
    };
    const ExitPrimitive = /* @__PURE__ */ __name(function(value) {
      this[args] = value;
    }, "ExitPrimitive");
    ExitPrimitive.prototype = Proto3;
    return function(value) {
      return new ExitPrimitive(value);
    };
  }, "makeExit");
  var exitSucceed = /* @__PURE__ */ makeExit({
    op: "Success",
    prop: "value",
    [evaluate](fiber2) {
      const cont = fiber2.getCont(contA);
      return cont ? cont[contA](this[args], fiber2, this) : fiber2.yieldWith(this);
    }
  });
  var StackTraceKey = {
    key: "effect/Cause/StackTrace"
  };
  var InterruptorStackTrace = {
    key: "effect/Cause/InterruptorStackTrace"
  };
  var exitFailCause = /* @__PURE__ */ makeExit({
    op: "Failure",
    prop: "cause",
    [evaluate](fiber2) {
      let cause = this[args];
      let annotated = false;
      if (fiber2.cache.stackFrame) {
        cause = causeAnnotate(cause, {
          mapUnsafe: /* @__PURE__ */ new Map([[StackTraceKey.key, fiber2.cache.stackFrame]])
        });
        annotated = true;
      }
      let cont = fiber2.getCont(contE);
      const interruptedCause = fiber2._interruptedCause;
      if (interruptedCause && fiber2.interruptible) {
        let skippedHandler = false;
        while (cont && fiber2.interruptible) {
          skippedHandler ||= identifier in cont;
          cont = fiber2.getCont(contE);
        }
        if (skippedHandler) {
          cause = causeFromReasons(cause.reasons.filter((reason) => reason._tag !== "Fail"));
        }
        cause = causeCombine(cause, interruptedCause);
        annotated = true;
      }
      return cont ? cont[contE](cause, fiber2, annotated ? void 0 : this) : fiber2.yieldWith(annotated ? exitFailCause(cause) : this);
    }
  });
  var exitFail = /* @__PURE__ */ __name((e) => exitFailCause(causeFail(e)), "exitFail");
  var exitDie = /* @__PURE__ */ __name((defect2) => exitFailCause(causeDie(defect2)), "exitDie");
  var withFiber = /* @__PURE__ */ makePrimitive({
    op: "WithFiber",
    [evaluate](fiber2) {
      return this[args](fiber2);
    }
  });
  var withFiberSucceed = /* @__PURE__ */ makePrimitive({
    op: "WithFiberSucceed",
    [evaluate](fiber2) {
      const value = this[args](fiber2);
      const cont = fiber2.getCont(contA);
      return cont ? cont[contA](value, fiber2) : fiber2.yieldWith(exitSucceed(value));
    }
  });
  var YieldableError = /* @__PURE__ */ (function() {
    class YieldableError2 extends globalThis.Error {
      static {
        __name(this, "YieldableError");
      }
    }
    const proto = /* @__PURE__ */ makePrimitiveProto({
      op: "YieldableError",
      [evaluate]() {
        return exitFail(this);
      }
    });
    delete proto.toString;
    Object.assign(YieldableError2.prototype, proto);
    return YieldableError2;
  })();
  var Error2 = /* @__PURE__ */ (function() {
    const plainArgsSymbol = /* @__PURE__ */ Symbol.for("effect/Data/Error/plainArgs");
    return class Base extends YieldableError {
      static {
        __name(this, "Base");
      }
      constructor(args2) {
        super(args2?.message, args2?.cause ? {
          cause: args2.cause
        } : void 0);
        if (args2) {
          assignProperties(this, args2);
          Object.defineProperty(this, plainArgsSymbol, {
            value: args2,
            enumerable: false
          });
        }
      }
      toJSON() {
        return {
          ...this[plainArgsSymbol],
          ...this
        };
      }
    };
  })();
  var TaggedError = /* @__PURE__ */ __name((tag2) => {
    class Base extends Error2 {
      static {
        __name(this, "Base");
      }
      _tag = tag2;
    }
    ;
    Base.prototype.name = tag2;
    return Base;
  }, "TaggedError");
  var NoSuchElementErrorTypeId = "~effect/Cause/NoSuchElementError";
  var NoSuchElementError = class extends (/* @__PURE__ */ TaggedError("NoSuchElementError")) {
    static {
      __name(this, "NoSuchElementError");
    }
    [NoSuchElementErrorTypeId] = NoSuchElementErrorTypeId;
    constructor(message2) {
      super({
        message: message2
      });
    }
  };
  var DoneTypeId = "~effect/Cause/Done";
  var isDone = /* @__PURE__ */ __name((u) => hasProperty(u, DoneTypeId), "isDone");
  var DoneVoid = {
    [DoneTypeId]: DoneTypeId,
    _tag: "Done",
    value: void 0
  };
  var Done = /* @__PURE__ */ __name((value) => {
    if (value === void 0) return DoneVoid;
    return {
      [DoneTypeId]: DoneTypeId,
      _tag: "Done",
      value
    };
  }, "Done");
  var doneVoid = /* @__PURE__ */ exitFail(DoneVoid);
  var done = /* @__PURE__ */ __name((value) => {
    if (value === void 0) return doneVoid;
    return exitFail(Done(value));
  }, "done");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Effectable.js
  var Prototype2 = /* @__PURE__ */ __name((options) => makePrimitiveProto({
    op: options.label,
    [evaluate]: options.evaluate
  }), "Prototype");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/option.js
  var TypeId = "~effect/Option";
  var CommonProto = {
    [TypeId]: {
      _A: /* @__PURE__ */ __name((_) => _, "_A")
    },
    ...PipeInspectableProto,
    [Symbol.iterator]() {
      return new SingleShotGen(this);
    }
  };
  var SomeProto = /* @__PURE__ */ Object.defineProperty(/* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto), {
    _tag: "Some",
    _op: "Some",
    [symbol2](that) {
      return isOption(that) && isSome(that) && equals(this.value, that.value);
    },
    [symbol]() {
      return combine(hash(this._tag))(hash(this.value));
    },
    toString() {
      return `some(${format2(this.value)})`;
    },
    toJSON() {
      return {
        _id: "Option",
        _tag: this._tag,
        value: toJson(this.value)
      };
    }
  }), "valueOrUndefined", {
    get() {
      return this.value;
    }
  });
  var NoneHash = /* @__PURE__ */ hash("None");
  var NoneProto = /* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto), {
    _tag: "None",
    _op: "None",
    valueOrUndefined: void 0,
    [symbol2](that) {
      return isOption(that) && isNone(that);
    },
    [symbol]() {
      return NoneHash;
    },
    toString() {
      return `none()`;
    },
    toJSON() {
      return {
        _id: "Option",
        _tag: this._tag
      };
    }
  });
  var isOption = /* @__PURE__ */ __name((input) => hasProperty(input, TypeId), "isOption");
  var isNone = /* @__PURE__ */ __name((fa) => fa._tag === "None", "isNone");
  var isSome = /* @__PURE__ */ __name((fa) => fa._tag === "Some", "isSome");
  var none = /* @__PURE__ */ Object.create(NoneProto);
  var SomeImpl = /* @__PURE__ */ __name(function(value) {
    this.value = value;
  }, "SomeImpl");
  SomeImpl.prototype = SomeProto;
  var some = /* @__PURE__ */ __name((value) => new SomeImpl(value), "some");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/result.js
  var TypeId2 = "~effect/Result";
  var CommonProto2 = {
    [TypeId2]: {
      /* v8 ignore next 2 */
      _A: /* @__PURE__ */ __name((_) => _, "_A"),
      _E: /* @__PURE__ */ __name((_) => _, "_E")
    },
    ...PipeInspectableProto,
    [Symbol.iterator]() {
      return new SingleShotGen(this);
    }
  };
  var SuccessProto = /* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto2), {
    _tag: "Success",
    _op: "Success",
    [symbol2](that) {
      return isResult(that) && isSuccess(that) && equals(this.success, that.success);
    },
    [symbol]() {
      return combine(hash(this._tag))(hash(this.success));
    },
    toString() {
      return `success(${format2(this.success)})`;
    },
    toJSON() {
      return {
        _id: "Result",
        _tag: this._tag,
        value: toJson(this.success)
      };
    }
  });
  var FailureProto = /* @__PURE__ */ Object.assign(/* @__PURE__ */ Object.create(CommonProto2), {
    _tag: "Failure",
    _op: "Failure",
    [symbol2](that) {
      return isResult(that) && isFailure(that) && equals(this.failure, that.failure);
    },
    [symbol]() {
      return combine(hash(this._tag))(hash(this.failure));
    },
    toString() {
      return `failure(${format2(this.failure)})`;
    },
    toJSON() {
      return {
        _id: "Result",
        _tag: this._tag,
        failure: toJson(this.failure)
      };
    }
  });
  var isResult = /* @__PURE__ */ __name((input) => hasProperty(input, TypeId2), "isResult");
  var isFailure = /* @__PURE__ */ __name((result2) => result2._tag === "Failure", "isFailure");
  var isSuccess = /* @__PURE__ */ __name((result2) => result2._tag === "Success", "isSuccess");
  var FailureImpl = /* @__PURE__ */ __name(function(failure2) {
    this.failure = failure2;
  }, "FailureImpl");
  FailureImpl.prototype = FailureProto;
  var fail = /* @__PURE__ */ __name((failure2) => new FailureImpl(failure2), "fail");
  var SuccessImpl = /* @__PURE__ */ __name(function(success) {
    this.success = success;
  }, "SuccessImpl");
  SuccessImpl.prototype = SuccessProto;
  var succeed = /* @__PURE__ */ __name((success) => new SuccessImpl(success), "succeed");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Order.js
  function make(compare) {
    return (self, that) => self === that ? 0 : compare(self, that);
  }
  __name(make, "make");
  var Number2 = /* @__PURE__ */ make((self, that) => {
    if (globalThis.Number.isNaN(self) && globalThis.Number.isNaN(that)) return 0;
    if (globalThis.Number.isNaN(self)) return -1;
    if (globalThis.Number.isNaN(that)) return 1;
    return self < that ? -1 : 1;
  });
  var mapInput = /* @__PURE__ */ dual(2, (self, f) => make((b1, b2) => self(f(b1), f(b2))));
  var isLessThan = /* @__PURE__ */ __name((O) => dual(2, (self, that) => O(self, that) === -1), "isLessThan");
  var isGreaterThan = /* @__PURE__ */ __name((O) => dual(2, (self, that) => O(self, that) === 1), "isGreaterThan");
  var isLessThanOrEqualTo = /* @__PURE__ */ __name((O) => dual(2, (self, that) => O(self, that) !== 1), "isLessThanOrEqualTo");
  var isGreaterThanOrEqualTo = /* @__PURE__ */ __name((O) => dual(2, (self, that) => O(self, that) !== -1), "isGreaterThanOrEqualTo");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Option.js
  var none2 = /* @__PURE__ */ __name(() => none, "none");
  var some2 = some;
  var isOption2 = isOption;
  var isNone2 = isNone;
  var isSome2 = isSome;
  var match = /* @__PURE__ */ dual(2, (self, {
    onNone,
    onSome
  }) => isNone2(self) ? onNone() : onSome(self.value));
  var getOrElse = /* @__PURE__ */ dual(2, (self, onNone) => isNone2(self) ? onNone() : self.value);
  var getOrNull = /* @__PURE__ */ getOrElse(constNull);
  var getOrUndefined = /* @__PURE__ */ getOrElse(constUndefined);
  var liftThrowable = /* @__PURE__ */ __name((f) => (...a) => {
    try {
      return some2(f(...a));
    } catch {
      return none2();
    }
  }, "liftThrowable");
  var map = /* @__PURE__ */ dual(2, (self, f) => isNone2(self) ? none2() : some2(f(self.value)));
  var flatMap = /* @__PURE__ */ dual(2, (self, f) => isNone2(self) ? none2() : f(self.value));
  var flatten = /* @__PURE__ */ flatMap(identity);

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Context.js
  var ServiceTypeId = "~effect/Context/Service";
  var Service = /* @__PURE__ */ __name(function() {
    function KeyClass() {
    }
    __name(KeyClass, "KeyClass");
    const self = KeyClass;
    Object.setPrototypeOf(self, ServiceProto);
    const init = /* @__PURE__ */ __name((key, options) => {
      self.key = key;
      if (options?.defaultValue) {
        self[ReferenceTypeId] = ReferenceTypeId;
        self.defaultValue = options.defaultValue;
      }
      if (options?.make) {
        ;
        self.make = options.make;
      }
      if (options?.fiberCached) {
        cacheKeys.add(key);
      }
      return self;
    }, "init");
    return arguments.length > 0 ? init(arguments[0], arguments[1]) : init;
  }, "Service");
  var ServiceProto = {
    [ServiceTypeId]: ServiceTypeId,
    .../* @__PURE__ */ Prototype2({
      label: "Service",
      evaluate(fiber2) {
        return exitSucceed(get(fiber2.context, this));
      }
    }),
    toJSON() {
      return {
        _id: "Service",
        key: this.key
      };
    },
    of(self) {
      return self;
    },
    context(self) {
      return make2(this, self);
    },
    use(f) {
      return withFiber((fiber2) => f(get(fiber2.context, this)));
    },
    useSync(f) {
      return withFiber((fiber2) => exitSucceed(f(get(fiber2.context, this))));
    }
  };
  var cacheKeys = /* @__PURE__ */ new Set();
  var ReferenceTypeId = "~effect/Context/Reference";
  var TypeId3 = "~effect/Context";
  var MaxDepth = 8;
  var FlattenAfterBaseHits = 8;
  var makeImpl = /* @__PURE__ */ __name((cacheRoot, base, overlay, depth) => {
    const self = Object.create(Proto);
    self.cacheRoot = cacheRoot ?? self;
    self.base = base;
    self.overlay = overlay;
    self.depth = depth;
    self._flat = void 0;
    self.baseHits = 0;
    return self;
  }, "makeImpl");
  var applyOverlays = /* @__PURE__ */ __name((map8, overlay) => {
    if (!overlay) return;
    applyOverlays(map8, overlay.parent);
    map8.set(overlay.key, overlay.value);
  }, "applyOverlays");
  var flatten2 = /* @__PURE__ */ __name((self) => {
    if (self._flat) return self._flat;
    if (!self.overlay) return self._flat = self.base;
    const map8 = new Map(self.base);
    applyOverlays(map8, self.overlay);
    return self._flat = map8;
  }, "flatten");
  var withFlat = /* @__PURE__ */ __name((self, f) => {
    const map8 = new Map(self.mapUnsafe);
    f(map8);
    return makeUnsafe(map8);
  }, "withFlat");
  var notFound = /* @__PURE__ */ Symbol();
  var lookup = /* @__PURE__ */ __name((self, key) => {
    const impl = self;
    for (let overlay = impl.overlay; overlay; overlay = overlay.parent) {
      if (overlay.key === key) return overlay.value;
    }
    const value = impl.base.get(key);
    if (value === void 0 && !impl.base.has(key)) return notFound;
    if (impl.overlay && ++impl.baseHits >= impl.base.size && impl.baseHits >= FlattenAfterBaseHits) {
      impl.base = flatten2(impl);
      impl.overlay = void 0;
      impl.depth = 0;
    }
    return value;
  }, "lookup");
  var makeUnsafe = /* @__PURE__ */ __name((mapUnsafe) => makeImpl(void 0, mapUnsafe, void 0, 0), "makeUnsafe");
  var Proto = {
    get mapUnsafe() {
      return flatten2(this);
    },
    ...PipeInspectableProto,
    [TypeId3]: {
      _Services: /* @__PURE__ */ __name((_) => _, "_Services")
    },
    toJSON() {
      return {
        _id: "Context",
        services: Array.from(this.mapUnsafe).map(([key, value]) => ({
          key,
          value
        }))
      };
    },
    [symbol2](that) {
      if (!isContext(that)) return false;
      const self = this.mapUnsafe;
      const other = that.mapUnsafe;
      if (self.size !== other.size) return false;
      for (const [key, value] of self) {
        if (!other.has(key) || !equals(value, other.get(key))) return false;
      }
      return true;
    },
    [symbol]() {
      return number(this.mapUnsafe.size);
    }
  };
  var hasSameCache = /* @__PURE__ */ __name((self, that) => self.cacheRoot === that.cacheRoot, "hasSameCache");
  var isContext = /* @__PURE__ */ __name((u) => hasProperty(u, TypeId3), "isContext");
  var isReference = /* @__PURE__ */ __name((u) => !!u[ReferenceTypeId], "isReference");
  var empty = /* @__PURE__ */ __name(() => emptyContext2, "empty");
  var emptyContext2 = /* @__PURE__ */ makeUnsafe(/* @__PURE__ */ new Map());
  var make2 = /* @__PURE__ */ __name((key, service3) => makeUnsafe(/* @__PURE__ */ new Map([[key.key, service3]])), "make");
  var add = /* @__PURE__ */ dual(3, (self, key, service3) => addUnsafe(self, key.key, service3));
  var addUnsafe = /* @__PURE__ */ __name((self, key, service3) => {
    const impl = self;
    const cacheRoot = cacheKeys.has(key) ? void 0 : impl.cacheRoot;
    if (impl.depth >= MaxDepth) {
      const map8 = new Map(impl._flat ?? impl.base);
      if (!impl._flat) applyOverlays(map8, impl.overlay);
      map8.set(key, service3);
      return makeImpl(cacheRoot, map8, void 0, 0);
    }
    return makeImpl(cacheRoot, impl.base, {
      key,
      value: service3,
      parent: impl.overlay
    }, impl.depth + 1);
  }, "addUnsafe");
  var getOrUndefined2 = /* @__PURE__ */ dual(2, (self, key) => getOrUndefinedUnsafe(self, key.key));
  var getOrUndefinedUnsafe = /* @__PURE__ */ __name((self, key) => {
    const value = lookup(self, key);
    return value === notFound ? void 0 : value;
  }, "getOrUndefinedUnsafe");
  var getUnsafe = /* @__PURE__ */ dual(2, (self, service3) => {
    const value = lookup(self, service3.key);
    if (value === notFound) {
      if (isReference(service3)) return getDefaultValue(service3);
      throw serviceNotFoundError(service3);
    }
    return value;
  });
  var get = getUnsafe;
  var defaultValueCacheKey = "~effect/Context/defaultValue";
  var getDefaultValue = /* @__PURE__ */ __name((ref) => {
    if (defaultValueCacheKey in ref) {
      return ref[defaultValueCacheKey];
    }
    return ref[defaultValueCacheKey] = ref.defaultValue();
  }, "getDefaultValue");
  var serviceNotFoundError = /* @__PURE__ */ __name((service3) => {
    const error = new Error(`Service not found${service3.key ? `: ${String(service3.key)}` : ""}`);
    if (error.stack) {
      const lines = error.stack.split("\n");
      lines.splice(1, 3);
      error.stack = lines.join("\n");
    }
    return error;
  }, "serviceNotFoundError");
  var merge = /* @__PURE__ */ dual(2, (self, that) => {
    if (self.mapUnsafe.size === 0) return that;
    if (that.mapUnsafe.size === 0) return self;
    return withFlat(self, (map8) => that.mapUnsafe.forEach((value, key) => map8.set(key, value)));
  });
  var mergeAll = /* @__PURE__ */ __name((...ctxs) => {
    const map8 = /* @__PURE__ */ new Map();
    for (let i = 0; i < ctxs.length; i++) {
      ctxs[i].mapUnsafe.forEach((value, key) => {
        map8.set(key, value);
      });
    }
    return makeUnsafe(map8);
  }, "mergeAll");
  var Reference = Service;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Duration.js
  var TypeId4 = "~effect/Duration";
  var bigint0 = /* @__PURE__ */ BigInt(0);
  var bigint1 = /* @__PURE__ */ BigInt(1);
  var bigint2 = /* @__PURE__ */ BigInt(2);
  var bigint10 = /* @__PURE__ */ BigInt(10);
  var bigint1e3 = /* @__PURE__ */ BigInt(1e3);
  var roundTiesAwayFromZero = /* @__PURE__ */ __name((input) => BigInt(Math.sign(input) * Math.round(Math.abs(input))), "roundTiesAwayFromZero");
  var roundMillisToNanos = /* @__PURE__ */ __name((millis2) => roundTiesAwayFromZero(millis2 * 1e6), "roundMillisToNanos");
  var fromNanosNumber = /* @__PURE__ */ __name((nanos2) => make3(Number.isFinite(nanos2) ? roundTiesAwayFromZero(nanos2) : nanos2), "fromNanosNumber");
  var parseNanos = /* @__PURE__ */ __name((input, scale) => {
    const decimalIndex = input.indexOf(".");
    if (decimalIndex === -1) return BigInt(input) * scale;
    const isNegative = input[0] === "-";
    const fractional = input.slice(decimalIndex + 1);
    const fractionalScale = bigint10 ** BigInt(fractional.length);
    const scaled = (BigInt(input.slice(isNegative ? 1 : 0, decimalIndex)) * fractionalScale + BigInt(fractional)) * scale;
    return divideTiesAwayFromZero(isNegative ? -scaled : scaled, fractionalScale);
  }, "parseNanos");
  var divideTiesAwayFromZero = /* @__PURE__ */ __name((numerator, denominator) => {
    const absolute = numerator < bigint0 ? -numerator : numerator;
    const rounded = absolute / denominator + (absolute % denominator * bigint2 >= denominator ? bigint1 : bigint0);
    return numerator < bigint0 ? -rounded : rounded;
  }, "divideTiesAwayFromZero");
  var DURATION_REGEXP = /^(-?\d+(?:\.\d+)?)\s+(nanos?|micros?|millis?|seconds?|minutes?|hours?|days?|weeks?)$/;
  var fromInputUnsafe = /* @__PURE__ */ __name((input) => {
    switch (typeof input) {
      case "number":
        return millis(input);
      case "bigint":
        return nanos(input);
      case "string": {
        if (input === "Infinity") {
          return infinity;
        }
        if (input === "-Infinity") {
          return negativeInfinity;
        }
        const match6 = DURATION_REGEXP.exec(input);
        if (!match6) break;
        const [_, valueStr, unit] = match6;
        if (unit === "nano" || unit === "nanos") {
          return nanos(parseNanos(valueStr, bigint1));
        }
        if (unit === "micro" || unit === "micros") {
          return nanos(parseNanos(valueStr, bigint1e3));
        }
        const value = Number(valueStr);
        switch (unit) {
          case "milli":
          case "millis":
            return millis(value);
          case "second":
          case "seconds":
            return seconds(value);
          case "minute":
          case "minutes":
            return minutes(value);
          case "hour":
          case "hours":
            return hours(value);
          case "day":
          case "days":
            return days(value);
          case "week":
          case "weeks":
            return weeks(value);
        }
        break;
      }
      case "object": {
        if (input === null) break;
        if (TypeId4 in input) return input;
        if (Array.isArray(input)) {
          if (input.length !== 2 || !input.every(isNumber)) {
            return invalid(input);
          }
          if (Number.isNaN(input[0]) || Number.isNaN(input[1])) {
            return zero;
          }
          if (input[0] === -Infinity || input[1] === -Infinity) {
            return negativeInfinity;
          }
          if (input[0] === Infinity || input[1] === Infinity) {
            return infinity;
          }
          return fromNanosNumber(input[0] * 1e9 + input[1]);
        }
        const obj = input;
        let millis2 = 0;
        if (obj.weeks) millis2 += obj.weeks * 6048e5;
        if (obj.days) millis2 += obj.days * 864e5;
        if (obj.hours) millis2 += obj.hours * 36e5;
        if (obj.minutes) millis2 += obj.minutes * 6e4;
        if (obj.seconds) millis2 += obj.seconds * 1e3;
        if (obj.milliseconds) millis2 += obj.milliseconds;
        if (!obj.microseconds && !obj.nanoseconds) return make3(millis2);
        return fromNanosNumber(millis2 * 1e6 + (obj.microseconds ?? 0) * 1e3 + (obj.nanoseconds ?? 0));
      }
    }
    return invalid(input);
  }, "fromInputUnsafe");
  var invalid = /* @__PURE__ */ __name((input) => {
    throw new Error(`Invalid Input: ${input}`);
  }, "invalid");
  var fromInput = /* @__PURE__ */ liftThrowable(fromInputUnsafe);
  var zeroDurationValue = {
    _tag: "Millis",
    millis: 0
  };
  var infinityDurationValue = {
    _tag: "Infinity"
  };
  var negativeInfinityDurationValue = {
    _tag: "NegativeInfinity"
  };
  var DurationProto = {
    [TypeId4]: TypeId4,
    [symbol]() {
      switch (this.value._tag) {
        case "Millis": {
          const nanos2 = this.value.millis * 1e6;
          return Number.isFinite(nanos2) ? hash(roundTiesAwayFromZero(nanos2)) : number(this.value.millis);
        }
        case "Nanos":
          return hash(this.value.nanos);
        default:
          return structure(this.value);
      }
    },
    [symbol2](that) {
      return isDuration(that) && equals2(this, that);
    },
    toString() {
      switch (this.value._tag) {
        case "Infinity":
          return "Infinity";
        case "NegativeInfinity":
          return "-Infinity";
        case "Nanos":
          return `${this.value.nanos} nanos`;
        case "Millis":
          return `${this.value.millis} millis`;
      }
    },
    toJSON() {
      switch (this.value._tag) {
        case "Millis":
          return {
            _id: "Duration",
            _tag: "Millis",
            millis: this.value.millis
          };
        case "Nanos":
          return {
            _id: "Duration",
            _tag: "Nanos",
            nanos: String(this.value.nanos)
          };
        case "Infinity":
          return {
            _id: "Duration",
            _tag: "Infinity"
          };
        case "NegativeInfinity":
          return {
            _id: "Duration",
            _tag: "NegativeInfinity"
          };
      }
    },
    [NodeInspectSymbol]() {
      return this.toJSON();
    },
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var make3 = /* @__PURE__ */ __name((input) => {
    const duration = Object.create(DurationProto);
    if (typeof input === "number") {
      if (isNaN(input) || input === 0 || Object.is(input, -0)) {
        duration.value = zeroDurationValue;
      } else if (!Number.isFinite(input)) {
        duration.value = input > 0 ? infinityDurationValue : negativeInfinityDurationValue;
      } else if (!Number.isInteger(input)) {
        duration.value = {
          _tag: "Nanos",
          nanos: roundMillisToNanos(input)
        };
      } else {
        duration.value = {
          _tag: "Millis",
          millis: input
        };
      }
    } else if (input === bigint0) {
      duration.value = zeroDurationValue;
    } else {
      duration.value = {
        _tag: "Nanos",
        nanos: input
      };
    }
    return duration;
  }, "make");
  var isDuration = /* @__PURE__ */ __name((u) => hasProperty(u, TypeId4), "isDuration");
  var zero = /* @__PURE__ */ make3(0);
  var infinity = /* @__PURE__ */ make3(Infinity);
  var negativeInfinity = /* @__PURE__ */ make3(-Infinity);
  var nanos = /* @__PURE__ */ __name((nanos2) => make3(nanos2), "nanos");
  var millis = /* @__PURE__ */ __name((millis2) => make3(millis2), "millis");
  var seconds = /* @__PURE__ */ __name((seconds2) => make3(seconds2 * 1e3), "seconds");
  var minutes = /* @__PURE__ */ __name((minutes2) => make3(minutes2 * 6e4), "minutes");
  var hours = /* @__PURE__ */ __name((hours2) => make3(hours2 * 36e5), "hours");
  var days = /* @__PURE__ */ __name((days2) => make3(days2 * 864e5), "days");
  var weeks = /* @__PURE__ */ __name((weeks2) => make3(weeks2 * 6048e5), "weeks");
  var toMillis = /* @__PURE__ */ __name((self) => match2(fromInputUnsafe(self), {
    onMillis: identity,
    onNanos: /* @__PURE__ */ __name((nanos2) => Number(nanos2) / 1e6, "onNanos"),
    onInfinity: /* @__PURE__ */ __name(() => Infinity, "onInfinity"),
    onNegativeInfinity: /* @__PURE__ */ __name(() => -Infinity, "onNegativeInfinity")
  }), "toMillis");
  var toNanosUnsafe = /* @__PURE__ */ __name((input) => {
    const self = fromInputUnsafe(input);
    switch (self.value._tag) {
      case "Infinity":
      case "NegativeInfinity":
        throw new Error("Cannot convert infinite duration to nanos");
      case "Nanos":
        return self.value.nanos;
      case "Millis":
        return roundMillisToNanos(self.value.millis);
    }
  }, "toNanosUnsafe");
  var match2 = /* @__PURE__ */ dual(2, (self, options) => {
    switch (self.value._tag) {
      case "Millis":
        return options.onMillis(self.value.millis);
      case "Nanos":
        return options.onNanos(self.value.nanos);
      case "Infinity":
        return options.onInfinity();
      case "NegativeInfinity":
        return (options.onNegativeInfinity ?? options.onInfinity)();
    }
  });
  var matchPair = /* @__PURE__ */ dual(3, (self, that, options) => {
    if (self.value._tag === "Infinity" || self.value._tag === "NegativeInfinity" || that.value._tag === "Infinity" || that.value._tag === "NegativeInfinity") return options.onInfinity(self, that);
    if (self.value._tag === "Millis") {
      return that.value._tag === "Millis" ? options.onMillis(self.value.millis, that.value.millis) : options.onNanos(toNanosUnsafe(self), that.value.nanos);
    } else {
      return options.onNanos(self.value.nanos, toNanosUnsafe(that));
    }
  });
  var Order = /* @__PURE__ */ make((self, that) => matchPair(self, that, {
    onMillis: /* @__PURE__ */ __name((self2, that2) => self2 < that2 ? -1 : self2 > that2 ? 1 : 0, "onMillis"),
    onNanos: /* @__PURE__ */ __name((self2, that2) => self2 < that2 ? -1 : self2 > that2 ? 1 : 0, "onNanos"),
    onInfinity: /* @__PURE__ */ __name((self2, that2) => {
      if (self2.value._tag === that2.value._tag) return 0;
      if (self2.value._tag === "Infinity") return 1;
      if (self2.value._tag === "NegativeInfinity") return -1;
      if (that2.value._tag === "Infinity") return -1;
      return 1;
    }, "onInfinity")
  }));
  var Equivalence = /* @__PURE__ */ __name((self, that) => matchPair(self, that, {
    onMillis: /* @__PURE__ */ __name((self2, that2) => self2 === that2, "onMillis"),
    onNanos: /* @__PURE__ */ __name((self2, that2) => self2 === that2, "onNanos"),
    onInfinity: /* @__PURE__ */ __name((self2, that2) => self2.value._tag === that2.value._tag, "onInfinity")
  }), "Equivalence");
  var isLessThanOrEqualTo2 = /* @__PURE__ */ isLessThanOrEqualTo(Order);
  var equals2 = /* @__PURE__ */ dual(2, (self, that) => Equivalence(self, that));

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/array.js
  var isArrayNonEmpty = /* @__PURE__ */ __name((self) => self.length > 0, "isArrayNonEmpty");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/count.js
  var normalize = /* @__PURE__ */ __name((n) => n > 0 ? Math.floor(n) : 0, "normalize");
  var normalizeNonEmpty = /* @__PURE__ */ __name((n) => Math.max(1, normalize(n)), "normalizeNonEmpty");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Result.js
  var succeed2 = succeed;
  var fail2 = fail;
  var isFailure2 = isFailure;
  var isSuccess2 = isSuccess;
  var match3 = /* @__PURE__ */ dual(2, (self, {
    onFailure,
    onSuccess
  }) => isFailure2(self) ? onFailure(self.failure) : onSuccess(self.success));

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Array.js
  var Array2 = globalThis.Array;
  var fromIterable = /* @__PURE__ */ __name((collection) => Array2.isArray(collection) ? collection : Array2.from(collection), "fromIterable");
  var prepend = /* @__PURE__ */ dual(2, (self, head2) => [head2, ...self]);
  var append = /* @__PURE__ */ dual(2, (self, last) => [...self, last]);
  var isArray = Array2.isArray;
  var isArrayNonEmpty2 = isArrayNonEmpty;
  var isReadonlyArrayNonEmpty = isArrayNonEmpty;
  function isOutOfBounds(i, as3) {
    return !Number.isFinite(i) || i < 0 || i >= as3.length;
  }
  __name(isOutOfBounds, "isOutOfBounds");
  var getUnsafe2 = /* @__PURE__ */ dual(2, (self, index) => {
    const i = Math.floor(index);
    if (isOutOfBounds(i, self)) {
      throw new Error(`Index out of bounds: ${i}`);
    }
    return self[i];
  });
  var headNonEmpty = /* @__PURE__ */ getUnsafe2(0);
  var chop = /* @__PURE__ */ dual(2, (self, f) => {
    const input = fromIterable(self);
    if (isReadonlyArrayNonEmpty(input)) {
      const [b, rest] = f(input);
      const out = [b];
      let next = rest;
      while (isArrayNonEmpty(next)) {
        const [b2, rest2] = f(next);
        out.push(b2);
        next = rest2;
      }
      return out;
    }
    return [];
  });
  var splitAtNonEmpty = /* @__PURE__ */ dual(2, (self, n) => {
    const _n = normalizeNonEmpty(n);
    return _n >= self.length ? [copy(self), []] : [prepend(self.slice(1, _n), headNonEmpty(self)), self.slice(_n)];
  });
  var copy = /* @__PURE__ */ __name((self) => self.slice(), "copy");
  var chunksOf = /* @__PURE__ */ dual(2, (self, n) => {
    const input = fromIterable(self);
    if (isReadonlyArrayNonEmpty(input)) {
      return chop(input, splitAtNonEmpty(n));
    }
    return [];
  });
  var hashBucketsAdd = /* @__PURE__ */ __name((buckets, value) => {
    const hash2 = hash(value);
    const bucket = buckets.get(hash2);
    if (bucket === void 0) {
      buckets.set(hash2, [value]);
      return true;
    }
    for (const previous of bucket) {
      if (equals(previous, value)) {
        return false;
      }
    }
    bucket.push(value);
    return true;
  }, "hashBucketsAdd");
  var empty2 = /* @__PURE__ */ __name(() => [], "empty");
  var of = /* @__PURE__ */ __name((a) => [a], "of");
  var map2 = /* @__PURE__ */ dual(2, (self, f) => self.map(f));
  var dedupe = /* @__PURE__ */ __name((self) => {
    const input = fromIterable(self);
    if (input.length < 2) {
      return [...input];
    }
    const buckets = /* @__PURE__ */ new Map();
    const out = [];
    for (const value of input) {
      if (hashBucketsAdd(buckets, value)) {
        out.push(value);
      }
    }
    return out;
  }, "dedupe");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Scheduler.js
  var Scheduler = /* @__PURE__ */ Reference("effect/Scheduler", {
    fiberCached: true,
    defaultValue: /* @__PURE__ */ __name(() => new MixedScheduler(), "defaultValue")
  });
  var setMicrotask = /* @__PURE__ */ __name((f) => {
    let cancelled = false;
    Promise.resolve().then(() => {
      if (!cancelled) f();
    });
    return () => {
      cancelled = true;
    };
  }, "setMicrotask");
  var setTimer = "setImmediate" in globalThis ? (f) => {
    const timer = globalThis.setImmediate(f);
    return () => globalThis.clearImmediate(timer);
  } : (f) => {
    const timer = setTimeout(f, 0);
    return () => clearTimeout(timer);
  };
  var setImmediate = /* @__PURE__ */ __name((f) => {
    try {
      return setTimer(f);
    } catch {
      return setMicrotask(f);
    }
  }, "setImmediate");
  var PriorityBuckets = class {
    static {
      __name(this, "PriorityBuckets");
    }
    buckets = [];
    scheduleTask(task, priority) {
      const buckets = this.buckets;
      const len = buckets.length;
      let bucket;
      let index = 0;
      for (; index < len; index++) {
        if (buckets[index][0] > priority) break;
        bucket = buckets[index];
      }
      if (bucket && bucket[0] === priority) {
        bucket[1].push(task);
      } else if (index === len) {
        buckets.push([priority, [task]]);
      } else {
        buckets.splice(index, 0, [priority, [task]]);
      }
    }
    drain() {
      const buckets = this.buckets;
      this.buckets = [];
      return buckets;
    }
  };
  var MixedScheduler = class {
    static {
      __name(this, "MixedScheduler");
    }
    executionMode;
    setImmediate;
    constructor(executionMode = "async", setImmediateFn) {
      this.executionMode = executionMode;
      this.setImmediate = setImmediateFn ?? (executionMode === "sync" ? setMicrotask : setImmediate);
    }
    /**
     * Returns whether the fiber has reached its operation budget and should yield.
     *
     * **When to use**
     *
     * Use to decide whether a fiber should yield after consuming its current
     * operation budget.
     *
     * @since 2.0.0
     */
    shouldYield(fiber2) {
      return fiber2.currentOpCount >= fiber2.cache.maxOpsBeforeYield;
    }
    /**
     * Creates a dispatcher that schedules work through this scheduler.
     *
     * **When to use**
     *
     * Use when you need a standalone dispatcher from a scheduler instance, for
     * example in tests that enqueue tasks and then flush them deterministically.
     *
     * @since 4.0.0
     */
    makeDispatcher() {
      return new MixedSchedulerDispatcher(this.setImmediate);
    }
  };
  var MixedSchedulerDispatcher = class {
    static {
      __name(this, "MixedSchedulerDispatcher");
    }
    tasks = /* @__PURE__ */ new PriorityBuckets();
    running = void 0;
    setImmediate;
    constructor(setImmediateFn = setImmediate) {
      this.setImmediate = setImmediateFn;
    }
    /**
     * @since 2.0.0
     */
    scheduleTask(task, priority) {
      this.tasks.scheduleTask(task, priority);
      if (this.running === void 0) {
        this.running = this.setImmediate(this.afterScheduled);
      }
    }
    /**
     * @since 2.0.0
     */
    afterScheduled = /* @__PURE__ */ __name(() => {
      this.running = void 0;
      this.runTasks();
    }, "afterScheduled");
    /**
     * @since 2.0.0
     */
    runTasks() {
      const buckets = this.tasks.drain();
      for (let i = 0; i < buckets.length; i++) {
        const toRun = buckets[i][1];
        for (let j = 0; j < toRun.length; j++) {
          toRun[j]();
        }
      }
    }
    /**
     * @since 2.0.0
     */
    flush() {
      while (this.tasks.buckets.length > 0) {
        if (this.running !== void 0) {
          this.running();
          this.running = void 0;
        }
        this.runTasks();
      }
    }
  };
  var MaxOpsBeforeYield = /* @__PURE__ */ Reference("effect/Scheduler/MaxOpsBeforeYield", {
    fiberCached: true,
    defaultValue: /* @__PURE__ */ __name(() => 2048, "defaultValue")
  });
  var PreventSchedulerYield = /* @__PURE__ */ Reference("effect/Scheduler/PreventSchedulerYield", {
    fiberCached: true,
    defaultValue: /* @__PURE__ */ __name(() => false, "defaultValue")
  });

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Data.js
  var TaggedError2 = TaggedError;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/encoding/EncodingError.js
  var EncodingErrorTypeId = "~effect/encoding/EncodingError";
  var EncodingError = class extends (/* @__PURE__ */ TaggedError2("EncodingError")) {
    static {
      __name(this, "EncodingError");
    }
    /**
     * Marks this value as an encoding or decoding error for runtime guards.
     *
     * **When to use**
     *
     * Use to identify `EncodingError` instances through `isEncodingError`.
     *
     * @since 4.0.0
     */
    [EncodingErrorTypeId] = EncodingErrorTypeId;
  };

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/encoding/Hex.js
  var random2 = /* @__PURE__ */ __name((length) => {
    switch (length) {
      case 16:
        return random16();
      case 32:
        return random32();
      default: {
        let result2 = "";
        for (let i = length >>> 3; i > 0; i--) {
          result2 += random8();
        }
        return result2;
      }
    }
  }, "random");
  var hexCharCodes = /* @__PURE__ */ Uint8Array.from("0123456789abcdef", (c) => c.charCodeAt(0));
  var randomWord = /* @__PURE__ */ __name(() => Math.random() * 4294967296 >>> 0, "randomWord");
  var random8 = /* @__PURE__ */ __name(() => {
    const a = randomWord();
    return String.fromCharCode(hexCharCodes[a >>> 28], hexCharCodes[a >>> 24 & 15], hexCharCodes[a >>> 20 & 15], hexCharCodes[a >>> 16 & 15], hexCharCodes[a >>> 12 & 15], hexCharCodes[a >>> 8 & 15], hexCharCodes[a >>> 4 & 15], hexCharCodes[a & 15]);
  }, "random8");
  var random16 = /* @__PURE__ */ __name(() => {
    const a = randomWord();
    const b = randomWord();
    return String.fromCharCode(hexCharCodes[a >>> 28], hexCharCodes[a >>> 24 & 15], hexCharCodes[a >>> 20 & 15], hexCharCodes[a >>> 16 & 15], hexCharCodes[a >>> 12 & 15], hexCharCodes[a >>> 8 & 15], hexCharCodes[a >>> 4 & 15], hexCharCodes[a & 15], hexCharCodes[b >>> 28], hexCharCodes[b >>> 24 & 15], hexCharCodes[b >>> 20 & 15], hexCharCodes[b >>> 16 & 15], hexCharCodes[b >>> 12 & 15], hexCharCodes[b >>> 8 & 15], hexCharCodes[b >>> 4 & 15], hexCharCodes[b & 15]);
  }, "random16");
  var random32 = /* @__PURE__ */ __name(() => {
    const a = randomWord();
    const b = randomWord();
    const c = randomWord();
    const d = randomWord();
    return String.fromCharCode(hexCharCodes[a >>> 28], hexCharCodes[a >>> 24 & 15], hexCharCodes[a >>> 20 & 15], hexCharCodes[a >>> 16 & 15], hexCharCodes[a >>> 12 & 15], hexCharCodes[a >>> 8 & 15], hexCharCodes[a >>> 4 & 15], hexCharCodes[a & 15], hexCharCodes[b >>> 28], hexCharCodes[b >>> 24 & 15], hexCharCodes[b >>> 20 & 15], hexCharCodes[b >>> 16 & 15], hexCharCodes[b >>> 12 & 15], hexCharCodes[b >>> 8 & 15], hexCharCodes[b >>> 4 & 15], hexCharCodes[b & 15], hexCharCodes[c >>> 28], hexCharCodes[c >>> 24 & 15], hexCharCodes[c >>> 20 & 15], hexCharCodes[c >>> 16 & 15], hexCharCodes[c >>> 12 & 15], hexCharCodes[c >>> 8 & 15], hexCharCodes[c >>> 4 & 15], hexCharCodes[c & 15], hexCharCodes[d >>> 28], hexCharCodes[d >>> 24 & 15], hexCharCodes[d >>> 20 & 15], hexCharCodes[d >>> 16 & 15], hexCharCodes[d >>> 12 & 15], hexCharCodes[d >>> 8 & 15], hexCharCodes[d >>> 4 & 15], hexCharCodes[d & 15]);
  }, "random32");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Tracer.js
  var ParentSpanKey = "effect/Tracer/ParentSpan";
  var ParentSpan = class extends (/* @__PURE__ */ Service()(ParentSpanKey, {
    fiberCached: true
  })) {
    static {
      __name(this, "ParentSpan");
    }
  };
  var make4 = /* @__PURE__ */ __name((options) => options, "make");
  var DisablePropagation = /* @__PURE__ */ Reference("effect/Tracer/DisablePropagation", {
    defaultValue: constFalse
  });
  var CurrentTraceLevel = /* @__PURE__ */ Reference("effect/Tracer/CurrentTraceLevel", {
    defaultValue: /* @__PURE__ */ __name(() => "Info", "defaultValue")
  });
  var MinimumTraceLevel = /* @__PURE__ */ Reference("effect/Tracer/MinimumTraceLevel", {
    defaultValue: /* @__PURE__ */ __name(() => "All", "defaultValue")
  });
  var TracerKey = "effect/Tracer";
  var nativeTracer = /* @__PURE__ */ make4({
    span: /* @__PURE__ */ __name((options) => new NativeSpan(options), "span")
  });
  var NativeSpan = class {
    static {
      __name(this, "NativeSpan");
    }
    _tag = "Span";
    sampled;
    name;
    parent;
    annotations;
    links;
    startTime;
    kind;
    status;
    _traceId = void 0;
    _spanId = void 0;
    _attributes = void 0;
    _events = void 0;
    constructor(options) {
      this.name = options.name;
      this.parent = options.parent;
      this.annotations = options.annotations;
      this.links = options.links;
      this.startTime = options.startTime;
      this.kind = options.kind;
      this.sampled = options.sampled;
      this.status = {
        _tag: "Started",
        startTime: options.startTime
      };
    }
    get traceId() {
      return this._traceId ??= getOrUndefined(this.parent)?.traceId ?? random2(32);
    }
    get spanId() {
      return this._spanId ??= random2(16);
    }
    get attributes() {
      return this._attributes ??= /* @__PURE__ */ new Map();
    }
    get events() {
      return this._events ??= [];
    }
    end(endTime, exit3) {
      this.status = {
        _tag: "Ended",
        endTime,
        exit: exit3,
        startTime: this.status.startTime
      };
    }
    attribute(key, value) {
      this.attributes.set(key, value);
    }
    event(name, startTime, attributes) {
      this.events.push([name, startTime, attributes ?? {}]);
    }
    addLinks(links) {
      this.links.push(...links);
    }
  };

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/metric.js
  var FiberRuntimeMetricsKey = "effect/Metric/FiberRuntimeMetrics";

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/references.js
  var CurrentStackFrame = /* @__PURE__ */ Reference("effect/References/CurrentStackFrame", {
    fiberCached: true,
    defaultValue: constUndefined
  });
  var TracerEnabled = /* @__PURE__ */ Reference("effect/References/TracerEnabled", {
    fiberCached: true,
    defaultValue: constTrue
  });
  var TracerTimingEnabled = /* @__PURE__ */ Reference("effect/References/TracerTimingEnabled", {
    defaultValue: constTrue
  });
  var TracerSpanAnnotations = /* @__PURE__ */ Reference("effect/References/TracerSpanAnnotations", {
    defaultValue: /* @__PURE__ */ __name(() => ({}), "defaultValue")
  });
  var TracerSpanLinks = /* @__PURE__ */ Reference("effect/References/TracerSpanLinks", {
    defaultValue: /* @__PURE__ */ __name(() => [], "defaultValue")
  });
  var CurrentLogAnnotations = /* @__PURE__ */ Reference("effect/References/CurrentLogAnnotations", {
    defaultValue: /* @__PURE__ */ __name(() => ({}), "defaultValue")
  });
  var CurrentLogLevel = /* @__PURE__ */ Reference("effect/References/CurrentLogLevel", {
    fiberCached: true,
    defaultValue: /* @__PURE__ */ __name(() => "Info", "defaultValue")
  });
  var MinimumLogLevel = /* @__PURE__ */ Reference("effect/References/MinimumLogLevel", {
    fiberCached: true,
    defaultValue: /* @__PURE__ */ __name(() => "Info", "defaultValue")
  });
  var CurrentLogSpans = /* @__PURE__ */ Reference("effect/References/CurrentLogSpans", {
    defaultValue: /* @__PURE__ */ __name(() => [], "defaultValue")
  });

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/tracer.js
  var makeStackCleaner = /* @__PURE__ */ __name((line) => (stack) => {
    let cache2;
    return () => {
      if (cache2 !== void 0) return cache2;
      const trace = stack();
      if (!trace) return void 0;
      const lines = trace.split("\n");
      if (lines[line] !== void 0) {
        cache2 = lines[line].trim();
        return cache2;
      }
    };
  }, "makeStackCleaner");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/effect.js
  var Interrupt = class extends ReasonBase {
    static {
      __name(this, "Interrupt");
    }
    constructor(fiberId3, annotations = constEmptyAnnotations) {
      super("Interrupt", annotations, "Interrupted");
      this.fiberId = fiberId3;
    }
    toString() {
      return `Interrupt(${this.fiberId})`;
    }
    toJSON() {
      return {
        _tag: "Interrupt",
        fiberId: this.fiberId
      };
    }
    [symbol2](that) {
      return isInterruptReason(that) && this.fiberId === that.fiberId && this.annotations === that.annotations;
    }
    [symbol]() {
      return combine(string(`${this._tag}:${this.fiberId}`))(random(this.annotations));
    }
  };
  var causeInterrupt = /* @__PURE__ */ __name((fiberId3) => new CauseImpl([new Interrupt(fiberId3)]), "causeInterrupt");
  var findError = /* @__PURE__ */ __name((self) => {
    for (let i = 0; i < self.reasons.length; i++) {
      const reason = self.reasons[i];
      if (reason._tag === "Fail") {
        return succeed2(reason.error);
      }
    }
    return fail2(self);
  }, "findError");
  var hasDies = /* @__PURE__ */ __name((self) => self.reasons.some(isDieReason), "hasDies");
  var hasInterrupts = /* @__PURE__ */ __name((self) => self.reasons.some(isInterruptReason), "hasInterrupts");
  var causeFilterInterruptors = /* @__PURE__ */ __name((self) => {
    let interruptors;
    for (let i = 0; i < self.reasons.length; i++) {
      const f = self.reasons[i];
      if (f._tag !== "Interrupt") continue;
      interruptors ??= /* @__PURE__ */ new Set();
      if (f.fiberId !== void 0) {
        interruptors.add(f.fiberId);
      }
    }
    return interruptors ? succeed2(interruptors) : fail2(self);
  }, "causeFilterInterruptors");
  var hasInterruptsOnly = /* @__PURE__ */ __name((self) => self.reasons.length > 0 && self.reasons.every(isInterruptReason), "hasInterruptsOnly");
  var causeMap = /* @__PURE__ */ dual(2, (self, f) => {
    let hasFail = false;
    const failures = self.reasons.map((failure2) => {
      if (isFailReason(failure2)) {
        hasFail = true;
        return new Fail(f(failure2.error), failure2.annotations);
      }
      return failure2;
    });
    return hasFail ? causeFromReasons(failures) : self;
  });
  var causeSquash = /* @__PURE__ */ __name((self) => {
    let die5;
    for (const reason of self.reasons) {
      if (reason._tag === "Fail") return reason.error;
      if (reason._tag === "Die") die5 ??= reason;
    }
    if (die5 !== void 0) return die5.defect;
    if (self.reasons.length > 0) return new globalThis.Error("All fibers interrupted without error");
    return new globalThis.Error("Empty cause");
  }, "causeSquash");
  var causePrettyErrors = /* @__PURE__ */ __name((self, options) => {
    const errors = [];
    const interrupts = [];
    if (self.reasons.length === 0) return errors;
    const prevStackLimit = getStackTraceLimit();
    if (prevStackLimit !== 0) setStackTraceLimit(1);
    try {
      for (const failure2 of self.reasons) {
        if (failure2._tag === "Interrupt") {
          interrupts.push(failure2);
          continue;
        }
        errors.push(causePrettyError(failure2._tag === "Die" ? failure2.defect : failure2.error, failure2.annotations, options));
      }
      if (errors.length === 0) {
        const cause = new Error("The fiber was interrupted by:");
        cause.name = "InterruptCause";
        cause.stack = interruptCauseStack(cause, interrupts);
        const error = new globalThis.Error("All fibers interrupted without error", {
          cause
        });
        error.name = "InterruptError";
        error.stack = `${error.name}: ${error.message}`;
        errors.push(causePrettyError(error, interrupts[0].annotations, options));
      }
    } finally {
      if (prevStackLimit !== 0) setStackTraceLimit(prevStackLimit);
    }
    return errors;
  }, "causePrettyErrors");
  var causePrettyError = /* @__PURE__ */ __name((original, annotations, options) => {
    const kind = typeof original;
    let error;
    if (original && kind === "object") {
      error = new globalThis.Error(causePrettyMessage(original), {
        cause: original.cause ? causePrettyError(original.cause) : void 0
      });
      if (typeof original.name === "string") {
        error.name = original.name;
      }
      if (typeof original.stack === "string") {
        error.stack = cleanErrorStack(original.stack, error, annotations);
      } else {
        const stack = `${error.name}: ${error.message}`;
        error.stack = annotations ? addStackAnnotations(stack, annotations) : stack;
      }
      if (options?.includeCauseInStack) {
        error.stack = renderPrettyError(error);
      }
      for (const key of Object.keys(original)) {
        if (!(key in error)) {
          ;
          error[key] = original[key];
        }
      }
    } else {
      error = new globalThis.Error(!original ? `Unknown error: ${original}` : kind === "string" ? original : formatJson(original));
      const stack = `${error.name}: ${error.message}`;
      error.stack = annotations ? addStackAnnotations(stack, annotations) : stack;
    }
    return error;
  }, "causePrettyError");
  var causePrettyMessage = /* @__PURE__ */ __name((u) => {
    if (typeof u.message === "string") {
      return u.message;
    } else if (typeof u.toString === "function" && u.toString !== Object.prototype.toString && u.toString !== Array.prototype.toString) {
      try {
        return u.toString();
      } catch {
      }
    }
    return formatJson(u);
  }, "causePrettyMessage");
  var locationRegExp = /\((.*)\)/g;
  var cleanErrorStack = /* @__PURE__ */ __name((stack, error, annotations) => {
    const message2 = `${error.name}: ${error.message}`;
    const lines = (stack.startsWith(message2) ? stack.slice(message2.length) : stack).split("\n");
    const out = [message2];
    for (let i = 1; i < lines.length; i++) {
      if (/Generator\.next|~effect\/(?:Effect|Utils)/.test(lines[i])) {
        break;
      }
      out.push(lines[i]);
    }
    return annotations ? addStackAnnotations(out.join("\n"), annotations) : out.join("\n");
  }, "cleanErrorStack");
  var addStackAnnotations = /* @__PURE__ */ __name((stack, annotations) => {
    const frame = annotations?.get(StackTraceKey.key);
    if (frame) {
      stack = `${stack}
${currentStackTrace(frame)}`;
    }
    return stack;
  }, "addStackAnnotations");
  var interruptCauseStack = /* @__PURE__ */ __name((error, interrupts) => {
    const out = [`${error.name}: ${error.message}`];
    for (const current of interrupts) {
      const fiberId3 = current.fiberId !== void 0 ? `#${current.fiberId}` : "unknown";
      const frame = current.annotations.get(InterruptorStackTrace.key);
      out.push(`    at fiber (${fiberId3})`);
      if (frame) out.push(currentStackTrace(frame));
    }
    return out.join("\n");
  }, "interruptCauseStack");
  var currentStackTrace = /* @__PURE__ */ __name((frame) => {
    const out = [];
    let current = frame;
    let i = 0;
    while (current && i < 10) {
      const stack = current.stack();
      if (stack) {
        const locationMatchAll = stack.matchAll(locationRegExp);
        let match6 = false;
        for (const [, location] of locationMatchAll) {
          match6 = true;
          out.push(`    at ${current.name} (${location})`);
        }
        if (!match6) {
          out.push(`    at ${current.name} (${stack.replace(/^at /, "")})`);
        }
      } else {
        out.push(`    at ${current.name}`);
      }
      current = current.parent;
      i++;
    }
    return out.join("\n");
  }, "currentStackTrace");
  var causePretty = /* @__PURE__ */ __name((cause) => causePrettyErrors(cause).map(renderPrettyError).join("\n"), "causePretty");
  var renderPrettyError = /* @__PURE__ */ __name((e) => e.cause ? `${e.stack} {
${renderErrorCause(e.cause, "  ")}
}` : e.stack, "renderPrettyError");
  var renderErrorCause = /* @__PURE__ */ __name((cause, prefix) => {
    const lines = cause.stack.split("\n");
    let stack = `${prefix}[cause]: ${lines[0]}`;
    for (let i = 1, len = lines.length; i < len; i++) {
      stack += `
${prefix}${lines[i]}`;
    }
    if (cause.cause) {
      stack += ` {
${renderErrorCause(cause.cause, `${prefix}  `)}
${prefix}}`;
    }
    return stack;
  }, "renderErrorCause");
  var FiberTypeId = "~effect/Fiber";
  var fiberVariance = {
    _A: identity,
    _E: identity
  };
  var fiberIdStore = {
    id: 0
  };
  var AsyncResource = /* @__PURE__ */ (() => {
    try {
      return globalThis.process?.getBuiltinModule?.("node:async_hooks")?.AsyncResource;
    } catch {
      return void 0;
    }
  })();
  var captureAsyncContext = /* @__PURE__ */ __name(() => AsyncResource === void 0 ? void 0 : new AsyncResource("effect/Fiber"), "captureAsyncContext");
  var getCurrentFiber = /* @__PURE__ */ __name(() => globalThis[currentFiberTypeId], "getCurrentFiber");
  var FiberImpl = class {
    static {
      __name(this, "FiberImpl");
    }
    constructor(context3, interruptible2 = true) {
      this.setContext(context3);
      this.id = ++fiberIdStore.id;
      this.currentOpCount = 0;
      this.interruptible = interruptible2;
      this._stack = [];
      this._observers = void 0;
      this._exit = void 0;
      this._children = void 0;
      this._interruptedCause = void 0;
      this._yielded = void 0;
      this._running = false;
      this._deferredInterrupt = false;
      this._parent = void 0;
      this._asyncContext = void 0;
      this.cache.runtimeMetrics?.recordFiberStart(this.context);
    }
    get [FiberTypeId]() {
      return fiberVariance;
    }
    get currentDispatcher() {
      return this._dispatcher ??= this.cache.scheduler.makeDispatcher();
    }
    getRef(ref) {
      return get(this.context, ref);
    }
    addObserver(cb) {
      if (this._exit) {
        cb(this._exit);
        return constVoid;
      }
      if (this._observers === void 0) {
        this._observers = [cb];
      } else {
        this._observers.push(cb);
      }
      return () => this.removeObserver(cb);
    }
    removeObserver(cb) {
      if (this._exit || this._observers === void 0) return;
      const index = this._observers.indexOf(cb);
      if (index >= 0) {
        this._observers.splice(index, 1);
      }
    }
    interruptUnsafe(fiberId3, annotations) {
      if (this._exit) {
        return;
      }
      let cause = causeInterrupt(fiberId3);
      if (this.cache.stackFrame) {
        cause = causeAnnotate(cause, make2(StackTraceKey, this.cache.stackFrame));
      }
      if (annotations) {
        cause = causeAnnotate(cause, annotations);
      }
      this._interruptedCause = this._interruptedCause ? causeCombine(this._interruptedCause, cause) : cause;
      if (this.interruptible) {
        if (this._running) {
          this._deferredInterrupt = true;
        } else {
          this.evaluate(failCause(this._interruptedCause));
        }
      }
    }
    pollUnsafe() {
      return this._exit;
    }
    evaluate(effect2) {
      if (this._exit) {
        return;
      } else if (this._asyncContext !== void 0) {
        const asyncContext = this._asyncContext;
        this._asyncContext = void 0;
        return asyncContext.runInAsyncScope(this.evaluate, this, effect2);
      } else if (this._yielded !== void 0) {
        const yielded = this._yielded;
        this._yielded = void 0;
        yielded();
      }
      const exit3 = this.runLoop(effect2);
      if (exit3 === Yield) {
        this._asyncContext = captureAsyncContext();
        return;
      }
      const interruptChildren = fiberMiddleware.interruptChildren && fiberMiddleware.interruptChildren(this);
      if (interruptChildren !== void 0) {
        return this.evaluate(flatMap2(interruptChildren, () => exit3));
      }
      this._exit = exit3;
      this.cache.runtimeMetrics?.recordFiberEnd(this.context, this._exit);
      if (this._parent) {
        this._parent._children?.delete(this);
        this._parent = void 0;
      }
      if (this._observers !== void 0) {
        const observers = this._observers;
        this._observers = void 0;
        for (let i = 0; i < observers.length; i++) {
          observers[i](exit3);
        }
      }
      this._stack.length = 0;
      this._children = void 0;
      this.context = empty();
    }
    runLoop(effect2) {
      const prevFiber = globalThis[currentFiberTypeId];
      globalThis[currentFiberTypeId] = this;
      const prevRunning = this._running;
      this._running = true;
      let yielding = false;
      let current = effect2;
      this.currentOpCount = 0;
      try {
        while (true) {
          try {
            if (this._deferredInterrupt) {
              this._deferredInterrupt = false;
              current = failCause(this._interruptedCause);
            }
            this.currentOpCount++;
            const cache2 = this.cache;
            if (!yielding && !cache2.preventYield && cache2.scheduler.shouldYield(this)) {
              yielding = true;
              const prev = current;
              current = flatMap2(yieldNow, () => prev);
            }
            current = cache2.tracerContext ? cache2.tracerContext(current, this) : current[evaluate](this);
            if (current === Yield) {
              const yielded = this._yielded;
              if (ExitTypeId in yielded) {
                this._deferredInterrupt = false;
                this._yielded = void 0;
                return yielded;
              } else if (this._deferredInterrupt) {
                this._yielded = void 0;
                yielded();
                continue;
              }
              return Yield;
            }
          } catch (error) {
            if (!hasProperty(current, evaluate)) {
              return exitDie(`Fiber.runLoop: Not a valid effect: ${String(current)}`);
            }
            current = exitDie(error);
          }
        }
      } finally {
        this._running = prevRunning;
        globalThis[currentFiberTypeId] = prevFiber;
      }
    }
    getCont(symbol4) {
      if (this._deferredInterrupt) {
        this._deferredInterrupt = false;
        return deferredInterruptCont;
      }
      while (true) {
        const op = this._stack.pop();
        if (!op) return void 0;
        const all3 = op[contAll];
        if (all3 !== void 0) {
          const cont = all3.call(op, this);
          if (cont) {
            ;
            cont[symbol4] = cont;
            return cont;
          }
        }
        if (op[symbol4]) return op;
      }
    }
    // Passes a success value straight to the next continuation instead of
    // returning an Exit for the run loop to unwrap. Each call counts as an
    // operation, and every `maxInlineSteps`th call returns an Exit instead, so
    // continuations calling continuations never nest deeper than that.
    succeedWith(value) {
      if ((++this.currentOpCount & maxInlineSteps - 1) === 0) {
        return exitSucceed(value);
      }
      const cont = this.getCont(contA);
      return cont ? cont[contA](value, this) : this.yieldWith(exitSucceed(value));
    }
    yieldWith(value) {
      this._yielded = value;
      return Yield;
    }
    children() {
      return this._children ??= /* @__PURE__ */ new Set();
    }
    pipe() {
      return pipeArguments(this, arguments);
    }
    setContext(context3) {
      const previous = this.context;
      this.context = context3;
      if (previous !== void 0 && hasSameCache(previous, context3)) return;
      const root = context3.cacheRoot;
      const cache2 = root._fiberCache ??= makeFiberContextCache(context3);
      if (this.cache?.scheduler !== cache2.scheduler) {
        this._dispatcher = void 0;
      }
      this.cache = cache2;
    }
    get currentSpanLocal() {
      const span = this.cache.span;
      return span?._tag === "Span" ? span : void 0;
    }
  };
  var makeFiberContextCache = /* @__PURE__ */ __name((context3) => {
    const currentTracer = getOrUndefinedUnsafe(context3, TracerKey);
    return {
      scheduler: get(context3, Scheduler),
      tracer: currentTracer,
      tracerContext: currentTracer ? currentTracer["context"] : void 0,
      tracerEnabled: get(context3, TracerEnabled),
      span: getOrUndefinedUnsafe(context3, ParentSpanKey),
      logLevel: get(context3, CurrentLogLevel),
      minimumLogLevel: get(context3, MinimumLogLevel),
      stackFrame: get(context3, CurrentStackFrame),
      runtimeMetrics: getOrUndefinedUnsafe(context3, FiberRuntimeMetricsKey),
      maxOpsBeforeYield: get(context3, MaxOpsBeforeYield),
      preventYield: get(context3, PreventSchedulerYield)
    };
  }, "makeFiberContextCache");
  var deferredInterruptCont = {
    [contA](_value, fiber2) {
      return failCause(fiber2._interruptedCause);
    },
    [contE](_cause, fiber2) {
      return failCause(fiber2._interruptedCause);
    }
  };
  var maxInlineSteps = 32;
  var fiberMiddleware = {
    interruptChildren: void 0
  };
  var fiberStackAnnotations = /* @__PURE__ */ __name((fiber2) => {
    if (!fiber2.cache.stackFrame) return void 0;
    const annotations = /* @__PURE__ */ new Map();
    annotations.set(InterruptorStackTrace.key, fiber2.cache.stackFrame);
    return makeUnsafe(annotations);
  }, "fiberStackAnnotations");
  var fiberAwait = /* @__PURE__ */ __name((self) => {
    const impl = self;
    if (impl._exit) return succeed3(impl._exit);
    return callback((resume) => {
      if (impl._exit) return resume(succeed3(impl._exit));
      return sync(self.addObserver((exit3) => resume(succeed3(exit3))));
    });
  }, "fiberAwait");
  var fiberAwaitAll = /* @__PURE__ */ __name((self) => callback((resume) => {
    const iter = self[Symbol.iterator]();
    const exits = [];
    let cancel = void 0;
    function loop() {
      let result2 = iter.next();
      while (!result2.done) {
        if (result2.value._exit) {
          exits.push(result2.value._exit);
          result2 = iter.next();
          continue;
        }
        cancel = result2.value.addObserver((exit3) => {
          exits.push(exit3);
          loop();
        });
        return;
      }
      resume(succeed3(exits));
    }
    __name(loop, "loop");
    loop();
    return sync(() => cancel?.());
  }), "fiberAwaitAll");
  var fiberJoin = /* @__PURE__ */ __name((self) => {
    const impl = self;
    if (impl._exit) return impl._exit;
    return callback((resume) => {
      if (impl._exit) return resume(impl._exit);
      return sync(self.addObserver(resume));
    });
  }, "fiberJoin");
  var fiberInterrupt = /* @__PURE__ */ __name((self) => withFiber((fiber2) => fiberInterruptAs(self, fiber2.id)), "fiberInterrupt");
  var fiberInterruptAs = /* @__PURE__ */ dual((args2) => hasProperty(args2[0], FiberTypeId), (self, fiberId3, annotations) => withFiber((parent) => {
    let ann = fiberStackAnnotations(parent);
    ann = ann && annotations ? merge(ann, annotations) : ann ?? annotations;
    self.interruptUnsafe(fiberId3, ann);
    return asVoid(fiberAwait(self));
  }));
  var fiberInterruptAll = /* @__PURE__ */ __name((fibers) => withFiber((parent) => {
    const annotations = fiberStackAnnotations(parent);
    let fiberArr = empty2();
    for (const fiber2 of fibers) {
      fiber2.interruptUnsafe(parent.id, annotations);
      fiberArr.push(fiber2);
    }
    return asVoid(fiberAwaitAll(fiberArr));
  }), "fiberInterruptAll");
  var succeed3 = exitSucceed;
  var failCause = exitFailCause;
  var fail3 = exitFail;
  var sync = /* @__PURE__ */ makePrimitive({
    op: "Sync",
    [evaluate](fiber2) {
      const value = this[args]();
      const cont = fiber2.getCont(contA);
      return cont ? cont[contA](value, fiber2) : fiber2.yieldWith(exitSucceed(value));
    }
  });
  var suspend = /* @__PURE__ */ makePrimitive({
    op: "Suspend",
    [evaluate](_fiber) {
      return this[args]();
    }
  });
  var fromOption2 = /* @__PURE__ */ dual((args2) => args2.length >= 2 || isOption2(args2[0]), (option2, onNone) => isNone2(option2) ? fail3(onNone ? onNone() : new NoSuchElementError("Effect.fromOption: Option.none")) : succeed3(option2.value));
  var fromResult = /* @__PURE__ */ match3({
    onFailure: fail3,
    onSuccess: succeed3
  });
  var yieldNowWith = /* @__PURE__ */ makePrimitive({
    op: "Yield",
    [evaluate](fiber2) {
      let resumed = false;
      fiber2.currentDispatcher.scheduleTask(() => {
        if (resumed) return;
        fiber2.evaluate(exitVoid);
      }, this[args] ?? 0);
      return fiber2.yieldWith(() => {
        resumed = true;
      });
    }
  });
  var yieldNow = /* @__PURE__ */ yieldNowWith(0);
  var succeedNone = /* @__PURE__ */ succeed3(/* @__PURE__ */ none2());
  var failCauseSync = /* @__PURE__ */ __name((evaluate2) => suspend(() => failCause(evaluate2())), "failCauseSync");
  var die = /* @__PURE__ */ __name((defect2) => exitDie(defect2), "die");
  var failSync = /* @__PURE__ */ __name((error) => suspend(() => fail3(error())), "failSync");
  var void_ = /* @__PURE__ */ succeed3(void 0);
  var try_ = /* @__PURE__ */ __name((options) => {
    const evaluate2 = typeof options === "function" ? options : options.try;
    const catcher = typeof options === "function" ? (cause) => new UnknownError(cause, "An error occurred in Effect.try") : options.catch;
    return suspend(() => {
      try {
        return succeed3(evaluate2());
      } catch (err) {
        return fail3(catcher(err));
      }
    });
  }, "try_");
  var tryPromise = /* @__PURE__ */ __name((options) => {
    const f = typeof options === "function" ? options : options.try;
    const catcher = typeof options === "function" ? (cause) => new UnknownError(cause, "An error occurred in Effect.tryPromise") : options.catch;
    return callbackOptions(function(resume, signal) {
      const failWithCatch = /* @__PURE__ */ __name((cause) => suspend(() => {
        try {
          return fail3(internalCall(() => catcher(cause)));
        } catch (err) {
          return die(err);
        }
      }), "failWithCatch");
      try {
        f(signal).then((a) => resume(succeed3(a)), (e) => resume(failWithCatch(e)));
      } catch (err) {
        resume(failWithCatch(err));
      }
    }, f.length !== 0);
  }, "tryPromise");
  var withFiberId = /* @__PURE__ */ __name((f) => withFiber((fiber2) => f(fiber2.id)), "withFiberId");
  var fiberId = /* @__PURE__ */ withFiberId(succeed3);
  var callbackOptions = /* @__PURE__ */ (function() {
    const Proto3 = /* @__PURE__ */ makePrimitiveProto({
      op: "Async",
      [evaluate](fiber2) {
        let resumed = false;
        let yielded = false;
        const controller = this.withSignal ? new AbortController() : void 0;
        const onCancel = this.register.call(fiber2.cache.scheduler, (effect2) => {
          if (resumed) return;
          resumed = true;
          if (yielded) {
            fiber2.evaluate(effect2);
          } else {
            yielded = effect2;
          }
        }, controller?.signal);
        if (yielded !== false) return yielded;
        yielded = true;
        fiber2._yielded = () => {
          resumed = true;
        };
        if (controller === void 0 && onCancel === void 0) {
          return Yield;
        }
        fiber2._stack.push(asyncFinalizer(() => {
          resumed = true;
          controller?.abort();
          return onCancel ?? exitVoid;
        }));
        return Yield;
      }
    });
    const AsyncImpl = /* @__PURE__ */ __name(function(register2, withSignal) {
      this.register = register2;
      this.withSignal = withSignal;
    }, "AsyncImpl");
    AsyncImpl.prototype = Proto3;
    return function(register2, withSignal) {
      return new AsyncImpl(register2, withSignal);
    };
  })();
  var asyncFinalizer = /* @__PURE__ */ makePrimitive({
    op: "AsyncFinalizer",
    [contAll](fiber2) {
      if (fiber2.interruptible) {
        fiber2.interruptible = false;
        fiber2._stack.push(setInterruptibleTrue);
      }
    },
    [contE](cause, _fiber) {
      return hasInterrupts(cause) ? flatMap2(combineFinalizerCause(exitFailCause(cause), this[args]()), () => failCause(cause)) : failCause(cause);
    }
  });
  var callback = /* @__PURE__ */ __name((register2) => callbackOptions(register2, register2.length >= 2), "callback");
  var never = /* @__PURE__ */ callback(constVoid);
  var gen = /* @__PURE__ */ __name((...args2) => {
    if (args2.length === 1) {
      const body2 = args2[0];
      return suspend(() => fromIteratorUnsafe(body2()));
    }
    const [options, body] = args2;
    return suspend(() => fromIteratorUnsafe(body.call(options.self)));
  }, "gen");
  var fnUntraced = /* @__PURE__ */ __name((body, ...pipeables) => {
    const fn3 = pipeables.length === 0 ? function() {
      return suspend(() => fromIteratorUnsafe(body.apply(this, arguments)));
    } : function() {
      let effect2 = suspend(() => fromIteratorUnsafe(body.apply(this, arguments)));
      for (let i = 0; i < pipeables.length; i++) {
        effect2 = pipeables[i](effect2, ...arguments);
      }
      return effect2;
    };
    return defineFunctionLength(body.length, fn3);
  }, "fnUntraced");
  var defineFunctionLength = /* @__PURE__ */ __name((length, fn3) => Object.defineProperty(fn3, "length", {
    value: length,
    configurable: true
  }), "defineFunctionLength");
  var fnStackCleaner = /* @__PURE__ */ makeStackCleaner(2);
  var fn = /* @__PURE__ */ __name(function() {
    const nameFirst = typeof arguments[0] === "string";
    const name = nameFirst ? arguments[0] : "Effect.fn";
    const spanOptions = nameFirst ? arguments[1] : void 0;
    const prevLimit = getStackTraceLimit();
    let defError;
    if (prevLimit !== 0) {
      setStackTraceLimit(2);
      defError = new globalThis.Error();
      setStackTraceLimit(prevLimit);
    }
    if (nameFirst) {
      return (body, ...pipeables) => makeFn(name, body, defError, pipeables, nameFirst, spanOptions);
    }
    return makeFn(name, arguments[0], defError, Array.prototype.slice.call(arguments, 1), nameFirst, spanOptions);
  }, "fn");
  var makeFn = /* @__PURE__ */ __name((name, bodyOrOptions, defError, pipeables, addSpan, spanOptions) => {
    const body = typeof bodyOrOptions === "function" ? bodyOrOptions : pipeables.shift().bind(bodyOrOptions.self);
    const definitionName = `${name} (definition)`;
    const definitionStack = defError ? fnStackCleaner(() => defError.stack) : constUndefined;
    return defineFunctionLength(body.length, function(...args2) {
      let result2 = suspend(() => {
        const iter = body.apply(this, arguments);
        return isEffect(iter) ? iter : fromIteratorUnsafe(iter);
      });
      for (let i = 0; i < pipeables.length; i++) {
        result2 = pipeables[i](result2, ...args2);
      }
      if (!isEffect(result2)) {
        return result2;
      }
      const prevLimit = getStackTraceLimit();
      let callError;
      if (prevLimit !== 0) {
        setStackTraceLimit(2);
        callError = new globalThis.Error();
        setStackTraceLimit(prevLimit);
      }
      return updateService(addSpan ? useSpan(name, spanOptions, (span) => provideParentSpan(result2, span)) : result2, CurrentStackFrame, (prev) => ({
        name,
        stack: callError ? fnStackCleaner(() => callError.stack) : constUndefined,
        parent: {
          name: definitionName,
          stack: definitionStack,
          parent: prev
        }
      }));
    });
  }, "makeFn");
  var fnUntracedEager = /* @__PURE__ */ __name((body, ...pipeables) => defineFunctionLength(body.length, pipeables.length === 0 ? function() {
    return fromIteratorEagerUnsafe(() => body.apply(this, arguments));
  } : function() {
    let effect2 = fromIteratorEagerUnsafe(() => body.apply(this, arguments));
    for (const pipeable of pipeables) {
      effect2 = pipeable(effect2, ...arguments);
    }
    return effect2;
  }), "fnUntracedEager");
  var fromIteratorEagerUnsafe = /* @__PURE__ */ __name((evaluate2) => {
    try {
      const iterator = evaluate2();
      let value = void 0;
      while (true) {
        const state = iterator.next(value);
        if (state.done) {
          return succeed3(state.value);
        }
        const primitive2 = state.value;
        if (primitive2 && primitive2._tag === "Success") {
          value = primitive2.value;
          continue;
        } else if (primitive2 && primitive2._tag === "Failure") {
          return state.value;
        } else {
          let isFirstExecution = true;
          return suspend(() => {
            if (isFirstExecution) {
              isFirstExecution = false;
              return flatMap2(state.value, (value2) => fromIteratorUnsafe(iterator, value2));
            } else {
              return suspend(() => fromIteratorUnsafe(evaluate2()));
            }
          });
        }
      }
    } catch (error) {
      return die(error);
    }
  }, "fromIteratorEagerUnsafe");
  var fromIteratorUnsafe = /* @__PURE__ */ (function() {
    const Proto3 = /* @__PURE__ */ makePrimitiveProto({
      op: "Iterator",
      [contA](value, fiber2) {
        const iter = this.iterator;
        while (true) {
          const state = iter.next(value);
          if (state.done) return succeed3(state.value);
          if (!effectIsExit(state.value)) {
            fiber2._stack.push(this);
            return state.value;
          } else if (state.value._tag === "Failure") {
            return state.value;
          }
          value = state.value.value;
        }
      },
      [evaluate](fiber2) {
        return this[contA](this.initial, fiber2);
      }
    });
    const IteratorImpl = /* @__PURE__ */ __name(function(iterator, initial) {
      this.iterator = iterator;
      this.initial = initial;
    }, "IteratorImpl");
    IteratorImpl.prototype = Proto3;
    return function(iterator, initial) {
      return new IteratorImpl(iterator, initial);
    };
  })();
  var as = /* @__PURE__ */ dual(2, (self, value) => new ContImpl(self, returnPayload, succeed3(value)));
  var evaluateCont = /* @__PURE__ */ __name(function(fiber2) {
    fiber2._stack.push(this);
    return this[args];
  }, "evaluateCont");
  var OnSuccessProto = /* @__PURE__ */ makePrimitiveProto({
    op: "OnSuccess",
    [evaluate]: evaluateCont
  });
  var ContImpl = /* @__PURE__ */ __name(function(self, cont, payload) {
    this[args] = self;
    this[contA] = cont;
    this.payload = payload;
  }, "ContImpl");
  ContImpl.prototype = OnSuccessProto;
  var returnPayload = /* @__PURE__ */ __name(function() {
    return this.payload;
  }, "returnPayload");
  var succeedPayload = /* @__PURE__ */ __name(function(_value, fiber2) {
    return fiber2.succeedWith(this.payload);
  }, "succeedPayload");
  var continuationMarksStack = /* @__PURE__ */ (() => {
    if (getStackTraceLimit() === 0) return false;
    const marker = "~effect/Effect/stackProbe";
    const probe = {
      [marker]: /* @__PURE__ */ __name(function stackProbe() {
        return new Error().stack;
      }, "stackProbe")
    };
    return probe[marker]()?.includes("[as " + marker + "]") === true;
  })();
  var mapCont = /* @__PURE__ */ __name(function(value, fiber2) {
    const f = this.payload;
    return fiber2.succeedWith(continuationMarksStack ? f(value) : internalCall(() => f(value)));
  }, "mapCont");
  var andThenCont = /* @__PURE__ */ __name(function(value) {
    const f = this.payload;
    return f(value);
  }, "andThenCont");
  var tapCont = /* @__PURE__ */ __name(function(value) {
    const f = this.payload;
    return new ContImpl(f(value), succeedPayload, value);
  }, "tapCont");
  var tapEffectCont = /* @__PURE__ */ __name(function(value) {
    return new ContImpl(this.payload, succeedPayload, value);
  }, "tapEffectCont");
  var asSome = /* @__PURE__ */ __name((self) => map3(self, some2), "asSome");
  var andThen = /* @__PURE__ */ dual(2, (self, f) => new ContImpl(self, isEffect(f) ? returnPayload : andThenCont, f));
  var tap = /* @__PURE__ */ dual(2, (self, f) => new ContImpl(self, isEffect(f) ? tapEffectCont : tapCont, f));
  var asVoid = /* @__PURE__ */ __name((self) => new ContImpl(self, returnPayload, exitVoid), "asVoid");
  var raceAllFirst = /* @__PURE__ */ __name((all3, options) => withFiber((parent) => {
    const fibers = /* @__PURE__ */ new Set();
    onExitUnsafe(parent, () => fibers.size === 0 ? void 0 : fiberInterruptAll(fibers));
    return callback((resume) => {
      let done4 = false;
      let i = 0;
      for (const effect2 of all3) {
        if (done4) break;
        const index = i++;
        const fiber2 = forkUnsafe(parent, effect2, true, true, false);
        fibers.add(fiber2);
        fiber2.addObserver((exit3) => {
          fibers.delete(fiber2);
          const isWinner = !done4;
          done4 = true;
          resume(exit3);
          if (isWinner && options?.onWinner) {
            options.onWinner({
              fiber: fiber2,
              index,
              parentFiber: parent
            });
          }
        });
      }
    });
  }), "raceAllFirst");
  var raceFirst = /* @__PURE__ */ dual((args2) => isEffect(args2[1]), (self, that, options) => raceAllFirst([self, that], options));
  var flatMap2 = /* @__PURE__ */ dual(2, (self, f) => new ContImpl(self, andThenCont, f));
  var effectIsExit = /* @__PURE__ */ __name((effect2) => effect2[ExitTypeId] !== void 0, "effectIsExit");
  var flatMapEager = /* @__PURE__ */ dual(2, (self, f) => {
    if (effectIsExit(self)) {
      return self._tag === "Success" ? f(self.value) : self;
    }
    return flatMap2(self, f);
  });
  var flatten3 = /* @__PURE__ */ __name((self) => flatMap2(self, identity), "flatten");
  var map3 = /* @__PURE__ */ dual(2, (self, f) => new ContImpl(self, mapCont, f));
  var mapEager = /* @__PURE__ */ dual(2, (self, f) => effectIsExit(self) ? exitMap(self, f) : map3(self, f));
  var mapErrorEager = /* @__PURE__ */ dual(2, (self, f) => effectIsExit(self) ? exitMapError(self, f) : mapError(self, f));
  var exitInterrupt = /* @__PURE__ */ __name((fiberId3) => exitFailCause(causeInterrupt(fiberId3)), "exitInterrupt");
  var exitIsSuccess = /* @__PURE__ */ __name((self) => self._tag === "Success", "exitIsSuccess");
  var exitIsFailure = /* @__PURE__ */ __name((self) => self._tag === "Failure", "exitIsFailure");
  var exitFilterCause = /* @__PURE__ */ __name((self) => self._tag === "Failure" ? succeed2(self.cause) : fail2(self), "exitFilterCause");
  var exitVoid = /* @__PURE__ */ exitSucceed(void 0);
  var exitMap = /* @__PURE__ */ dual(2, (self, f) => self._tag === "Success" ? exitSucceed(f(self.value)) : self);
  var exitMapError = /* @__PURE__ */ dual(2, (self, f) => {
    if (self._tag === "Success") return self;
    const error = findError(self.cause);
    if (isFailure2(error)) return self;
    return exitFail(f(error.success));
  });
  var exitZipRight = /* @__PURE__ */ dual(2, (self, that) => exitIsSuccess(self) ? that : self);
  var exitAsVoidAll = /* @__PURE__ */ __name((exits) => {
    const failures = [];
    for (const exit3 of exits) {
      if (exit3._tag === "Failure") {
        failures.push(...exit3.cause.reasons);
      }
    }
    return failures.length === 0 ? exitVoid : exitFailCause(causeFromReasons(failures));
  }, "exitAsVoidAll");
  var updateContext = /* @__PURE__ */ dual(2, (self, f) => withFiber((fiber2) => {
    const prevContext = fiber2.context;
    const nextContext = f(prevContext);
    if (prevContext === nextContext) return self;
    fiber2.setContext(nextContext);
    onExitUnsafe(fiber2, () => {
      fiber2.setContext(prevContext);
      return void 0;
    });
    return self;
  }));
  var updateService = /* @__PURE__ */ dual(3, (self, service3, f) => updateContext(self, (s) => {
    const prev = getUnsafe(s, service3);
    const next = f(prev);
    if (prev === next) return s;
    return add(s, service3, next);
  }));
  var context = /* @__PURE__ */ __name(() => getContext, "context");
  var getContext = /* @__PURE__ */ withFiber((fiber2) => succeed3(fiber2.context));
  var contextWith = /* @__PURE__ */ __name((f) => withFiber((fiber2) => f(fiber2.context)), "contextWith");
  var provideContext = /* @__PURE__ */ dual(2, (self, context3) => {
    if (effectIsExit(self)) return self;
    return updateContext(self, merge(context3));
  });
  var provideService = /* @__PURE__ */ __name(function() {
    if (arguments.length === 1) {
      return dual(2, (self, impl) => provideServiceImpl(self, arguments[0], impl));
    }
    return dual(3, (self, service3, impl) => provideServiceImpl(self, service3, impl)).apply(this, arguments);
  }, "provideService");
  var provideServiceImpl = /* @__PURE__ */ __name((self, service3, implementation) => updateContext(self, add(service3, implementation)), "provideServiceImpl");
  var zipWith = /* @__PURE__ */ dual((args2) => isEffect(args2[1]), (self, that, f, options) => options?.concurrent ? map3(all([self, that], {
    concurrency: 2
  }), ([a, a2]) => f(a, a2)) : flatMap2(self, (a) => map3(that, (a2) => f(a, a2))));
  var when = /* @__PURE__ */ dual(2, (self, condition) => flatMap2(condition, (pass) => pass ? asSome(self) : succeedNone));
  var forever = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self, options) => whileLoop({
    while: constTrue,
    body: constant(options?.disableYield ? self : flatMap2(self, (_) => yieldNow)),
    step: constVoid
  }));
  var catchCause = /* @__PURE__ */ dual(2, (self, f) => new OnFailureImpl(self, f.length !== 1 ? (cause) => f(cause) : f));
  var OnFailureProto = /* @__PURE__ */ makePrimitiveProto({
    op: "OnFailure",
    [evaluate]: evaluateCont
  });
  var OnFailureImpl = /* @__PURE__ */ __name(function(self, f) {
    this[args] = self;
    this[contE] = f;
  }, "OnFailureImpl");
  OnFailureImpl.prototype = OnFailureProto;
  var catchCauseFilter = /* @__PURE__ */ dual(3, (self, filter7, f) => catchCause(self, (cause) => {
    const eb = filter7(cause);
    return isFailure2(eb) ? failCause(eb.failure) : f(eb.success, cause);
  }));
  var catch_ = /* @__PURE__ */ dual(2, (self, f) => catchCauseFilter(self, findError, (e) => f(e)));
  var catchIf = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self, predicate, f, orElse) => catchCause(self, (cause) => {
    const error = findError(cause);
    if (isFailure2(error)) return failCause(error.failure);
    if (!predicate(error.success)) {
      return orElse ? orElse(error.success) : failCause(cause);
    }
    return f(error.success);
  }));
  var catchTag = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self, k, f, orElse) => {
    const pred = Array.isArray(k) ? (e) => hasProperty(e, "_tag") && k.includes(e._tag) : isTagged(k);
    return catchIf(self, pred, f, orElse);
  });
  var mapError = /* @__PURE__ */ dual(2, (self, f) => catch_(self, (error) => failSync(() => f(error))));
  var result = /* @__PURE__ */ __name((self) => matchEager(self, {
    onFailure: fail2,
    onSuccess: succeed2
  }), "result");
  var matchCauseEffect = /* @__PURE__ */ dual(2, (self, options) => new OnSuccessAndFailureImpl(self, options.onSuccess.length !== 1 ? (a) => options.onSuccess(a) : options.onSuccess, options.onFailure.length !== 1 ? (cause) => options.onFailure(cause) : options.onFailure));
  var OnSuccessAndFailureProto = /* @__PURE__ */ makePrimitiveProto({
    op: "OnSuccessAndFailure",
    [evaluate]: evaluateCont
  });
  var OnSuccessAndFailureImpl = /* @__PURE__ */ __name(function(self, onSuccess, onFailure) {
    this[args] = self;
    this[contA] = onSuccess;
    this[contE] = onFailure;
  }, "OnSuccessAndFailureImpl");
  OnSuccessAndFailureImpl.prototype = OnSuccessAndFailureProto;
  var matchSuccess = /* @__PURE__ */ __name(function(value, fiber2) {
    const handlers = this.payload;
    return fiber2.succeedWith(continuationMarksStack ? handlers.onSuccess(value) : internalCall(() => handlers.onSuccess(value)));
  }, "matchSuccess");
  var makeMatch = /* @__PURE__ */ __name((op, onFailure) => {
    const Proto3 = makePrimitiveProto({
      op,
      [evaluate]: evaluateCont,
      [contA]: matchSuccess,
      [contE]: onFailure
    });
    const MatchImpl2 = /* @__PURE__ */ __name(function(self, handlers) {
      this[args] = self;
      this.payload = handlers;
    }, "MatchImpl");
    MatchImpl2.prototype = Proto3;
    return MatchImpl2;
  }, "makeMatch");
  var MatchImpl = /* @__PURE__ */ makeMatch("Match", function(cause, fiber2) {
    const fail7 = cause.reasons.find(isFailReason);
    if (fail7 === void 0) return failCause(cause);
    const handlers = this.payload;
    return fiber2.succeedWith(continuationMarksStack ? handlers.onFailure(fail7.error) : internalCall(() => handlers.onFailure(fail7.error)));
  });
  var match4 = /* @__PURE__ */ dual(2, (self, options) => new MatchImpl(self, options));
  var matchEager = /* @__PURE__ */ dual(2, (self, options) => {
    if (effectIsExit(self)) {
      if (self._tag === "Success") return exitSucceed(options.onSuccess(self.value));
      const error = findError(self.cause);
      if (isFailure2(error)) return self;
      return exitSucceed(options.onFailure(error.success));
    }
    return match4(self, options);
  });
  var exit = /* @__PURE__ */ __name((self) => effectIsExit(self) ? exitSucceed(self) : exitPrimitive(self), "exit");
  var exitPrimitive = /* @__PURE__ */ makePrimitive({
    op: "Exit",
    [evaluate](fiber2) {
      fiber2._stack.push(this);
      return this[args];
    },
    [contA](value, fiber2, exit3) {
      return fiber2.succeedWith(exit3 ?? exitSucceed(value));
    },
    [contE](cause, fiber2, exit3) {
      return fiber2.succeedWith(exit3 ?? exitFailCause(cause));
    }
  });
  var timeoutOption = /* @__PURE__ */ dual(2, (self, duration) => raceFirst(asSome(self), as(sleep(duration), none2())));
  var ScopeTypeId = "~effect/Scope";
  var ScopeCloseableTypeId = "~effect/Scope/Closeable";
  var scopeTag = /* @__PURE__ */ Service("effect/Scope");
  var scopeClose = /* @__PURE__ */ __name((self, exit_) => withFiber((fiber2) => {
    fiberEnterUninterruptibleUnsafe(fiber2);
    return scopeCloseUnsafe(self, exit_) ?? void_;
  }), "scopeClose");
  var scopeCloseUnsafe = /* @__PURE__ */ __name((self, exit_) => {
    const state = self.state;
    if (state._tag === "Closed") return;
    self.state = {
      _tag: "Closed",
      exit: exit_
    };
    if (self.parent !== void 0) {
      scopeRemoveFinalizerUnsafe(self.parent, self);
    }
    if (state._tag === "Empty") return;
    if (state.finalizer !== void 0) {
      return runFinalizer(state.finalizer, exit_);
    }
    const finalizers = state.finalizers;
    if (finalizers.size === 1) {
      return runFinalizer(finalizers.values().next().value, exit_);
    }
    return scopeCloseFinalizers(self, finalizers, exit_);
  }, "scopeCloseUnsafe");
  var runFinalizer = /* @__PURE__ */ __name((finalizer, exit_) => {
    try {
      return finalizer(exit_);
    } catch (defect2) {
      return exitDie(defect2);
    }
  }, "runFinalizer");
  var combineFinalizerCause = /* @__PURE__ */ __name((exit_, finalizer) => exitIsSuccess(exit_) ? finalizer : catchCause(finalizer, (cause) => failCause(causeCombine(exit_.cause, cause))), "combineFinalizerCause");
  var scopeCloseFinalizers = /* @__PURE__ */ fnUntraced(function* (self, finalizers, exit_) {
    let exits = [];
    const fibers = [];
    const arr = Array.from(finalizers.values());
    const parent = getCurrentFiber();
    for (let i = arr.length - 1; i >= 0; i--) {
      const finalizer = arr[i];
      if (self.strategy === "sequential") {
        exits.push(yield* exit(runFinalizer(finalizer, exit_)));
      } else {
        fibers.push(forkUnsafe(parent, runFinalizer(finalizer, exit_), true, true, "inherit"));
      }
    }
    if (fibers.length > 0) {
      exits = yield* fiberAwaitAll(fibers);
    }
    return yield* exitAsVoidAll(exits);
  });
  var scopeForkUnsafe = /* @__PURE__ */ __name((scope3, finalizerStrategy) => {
    const child2 = makeScope(finalizerStrategy, scope3);
    if (scope3.state._tag === "Closed") {
      child2.state = scope3.state;
      return child2;
    }
    scopeAddFinalizerUnsafe(scope3, child2, (exit3) => scopeClose(child2, exit3));
    return child2;
  }, "scopeForkUnsafe");
  var scopeAddFinalizerExit = /* @__PURE__ */ __name((scope3, finalizer) => {
    return suspend(() => {
      if (scope3.state._tag === "Closed") {
        return finalizer(scope3.state.exit);
      }
      scopeAddFinalizerUnsafe(scope3, {}, finalizer);
      return void_;
    });
  }, "scopeAddFinalizerExit");
  var scopeAddFinalizer = /* @__PURE__ */ __name((scope3, finalizer) => scopeAddFinalizerExit(scope3, constant(finalizer)), "scopeAddFinalizer");
  var scopeAddFinalizerUnsafe = /* @__PURE__ */ __name((scope3, key, finalizer) => {
    if (scope3.state._tag === "Empty") {
      scope3.state = {
        _tag: "Open",
        finalizerKey: key,
        finalizer,
        finalizers: void 0
      };
    } else if (scope3.state._tag === "Open") {
      const state = scope3.state;
      if (state.finalizer !== void 0) {
        state.finalizers = /* @__PURE__ */ new Map([[state.finalizerKey, state.finalizer]]);
        state.finalizerKey = void 0;
        state.finalizer = void 0;
        state.finalizers.set(key, finalizer);
      } else {
        state.finalizers.set(key, finalizer);
      }
    }
  }, "scopeAddFinalizerUnsafe");
  var scopeRemoveFinalizerUnsafe = /* @__PURE__ */ __name((scope3, key) => {
    if (scope3.state._tag !== "Open") return;
    const state = scope3.state;
    if (state.finalizerKey === key) {
      scope3.state = constScopeEmpty;
    } else if (state.finalizers !== void 0) {
      state.finalizers.delete(key);
      if (state.finalizers.size === 0) {
        scope3.state = constScopeEmpty;
      }
    }
  }, "scopeRemoveFinalizerUnsafe");
  var scopeMakeUnsafe = /* @__PURE__ */ __name((finalizerStrategy) => makeScope(finalizerStrategy, void 0), "scopeMakeUnsafe");
  var makeScope = /* @__PURE__ */ __name((finalizerStrategy = "sequential", parent) => ({
    [ScopeCloseableTypeId]: ScopeCloseableTypeId,
    [ScopeTypeId]: ScopeTypeId,
    strategy: finalizerStrategy,
    parent,
    state: constScopeEmpty
  }), "makeScope");
  var constScopeEmpty = {
    _tag: "Empty"
  };
  var scopeMake = /* @__PURE__ */ __name((finalizerStrategy) => sync(() => scopeMakeUnsafe(finalizerStrategy)), "scopeMake");
  var scope = scopeTag;
  var provideScope = /* @__PURE__ */ provideService(scopeTag);
  var scoped = /* @__PURE__ */ __name((self) => withFiber((fiber2) => {
    const prev = fiber2.context;
    const scope3 = scopeMakeUnsafe();
    fiber2.setContext(add(fiber2.context, scopeTag, scope3));
    onExitUnsafe(fiber2, (exit3) => {
      fiber2.setContext(prev);
      return scopeCloseUnsafe(scope3, exit3);
    });
    return self;
  }), "scoped");
  var acquireRelease = /* @__PURE__ */ __name((acquire, release, options) => contextWith((context3) => uninterruptibleMask((restore) => flatMap2(scope, (scope3) => tap(options?.interruptible ? restore(acquire) : acquire, (a) => scopeAddFinalizerExit(scope3, (exit3) => provideContext(release(a, exit3), context3)))))), "acquireRelease");
  var addFinalizer = /* @__PURE__ */ __name((finalizer) => flatMap2(scope, (scope3) => contextWith((context3) => scopeAddFinalizerExit(scope3, (exit3) => provideContext(finalizer(exit3), context3)))), "addFinalizer");
  var OnExitImpl = /* @__PURE__ */ (function() {
    const Proto3 = /* @__PURE__ */ makePrimitiveProto({
      op: "OnExit",
      [evaluate](fiber2) {
        fiber2._stack.push(this);
        return this.effect;
      },
      [contAll](fiber2) {
        if (fiber2.interruptible && this.interruptible !== true) {
          fiber2._stack.push(setInterruptibleTrue);
          fiber2.interruptible = false;
        }
      },
      [contA](value, _, exit3) {
        exit3 ??= exitSucceed(value);
        let eff;
        try {
          eff = this.onExit(exit3);
        } catch (defect2) {
          eff = exitDie(defect2);
        }
        return eff ? flatMap2(eff, (_2) => exit3) : exit3;
      },
      [contE](cause, _, exit3) {
        exit3 ??= exitFailCause(cause);
        let eff;
        try {
          eff = this.onExit(exit3);
        } catch (defect2) {
          eff = exitDie(defect2);
        }
        return eff ? flatMap2(combineFinalizerCause(exit3, eff), (_2) => exit3) : exit3;
      }
    });
    const OnExitImpl2 = /* @__PURE__ */ __name(function(effect2, onExit4, interruptible2) {
      this.effect = effect2;
      this.onExit = onExit4;
      this.interruptible = interruptible2;
    }, "OnExitImpl");
    OnExitImpl2.prototype = Proto3;
    return OnExitImpl2;
  })();
  var onExitPrimitive = /* @__PURE__ */ __name((effect2, onExit4, interruptible2) => new OnExitImpl(effect2, onExit4, interruptible2), "onExitPrimitive");
  var onExitUnsafe = /* @__PURE__ */ __name((fiber2, f) => {
    ;
    fiber2._stack.push(new OnExitImpl(void 0, f, void 0));
  }, "onExitUnsafe");
  var onExit = /* @__PURE__ */ dual(2, onExitPrimitive);
  var ensuring = /* @__PURE__ */ dual(2, (self, finalizer) => onExit(self, (_) => finalizer));
  var onExitFilter = /* @__PURE__ */ dual(3, (self, filter7, f) => onExit(self, (exit3) => {
    const b = filter7(exit3);
    return isFailure2(b) ? void_ : f(b.success, exit3);
  }));
  var onError = /* @__PURE__ */ dual(2, (self, f) => onExitFilter(self, exitFilterCause, f));
  var onErrorFilter = /* @__PURE__ */ dual(3, (self, filter7, f) => onExit(self, (exit3) => {
    if (exit3._tag !== "Failure") {
      return void_;
    }
    const result2 = filter7(exit3.cause);
    return isFailure2(result2) ? void_ : f(result2.success, exit3.cause);
  }));
  var onInterrupt = /* @__PURE__ */ dual(2, (self, finalizer) => onErrorFilter(causeFilterInterruptors, finalizer)(self));
  var acquireUseRelease = /* @__PURE__ */ __name((acquire, use, release) => uninterruptibleMask((restore) => flatMap2(acquire, (a) => onExitPrimitive(suspend(() => restore(use(a))), (exit3) => release(a, exit3), true))), "acquireUseRelease");
  var interrupt = /* @__PURE__ */ withFiber((fiber2) => failCause(causeInterrupt(fiber2.id)));
  var uninterruptible = /* @__PURE__ */ __name((self) => withFiber((fiber2) => {
    if (!fiber2.interruptible) return self;
    fiber2.interruptible = false;
    fiber2._stack.push(setInterruptibleTrue);
    return self;
  }), "uninterruptible");
  var setInterruptible = /* @__PURE__ */ makePrimitive({
    op: "SetInterruptible",
    [contAll](fiber2) {
      fiber2.interruptible = this[args];
      if (fiber2._interruptedCause && fiber2.interruptible) {
        return () => failCause(fiber2._interruptedCause);
      }
    }
  });
  var setInterruptibleTrue = /* @__PURE__ */ setInterruptible(true);
  var setInterruptibleFalse = /* @__PURE__ */ setInterruptible(false);
  var setFiberInterruptible = /* @__PURE__ */ __name((fiber2) => {
    fiber2.interruptible = true;
    fiber2._stack.push(setInterruptibleFalse);
    if (fiber2._interruptedCause) return failCause(fiber2._interruptedCause);
  }, "setFiberInterruptible");
  var fiberEnterUninterruptibleUnsafe = /* @__PURE__ */ __name((fiber2) => {
    const impl = fiber2;
    if (!impl.interruptible) return;
    impl.interruptible = false;
    impl._stack.push(setInterruptibleTrue);
  }, "fiberEnterUninterruptibleUnsafe");
  var interruptible = /* @__PURE__ */ __name((self) => withFiber((fiber2) => {
    if (fiber2.interruptible) return self;
    return setFiberInterruptible(fiber2) ?? self;
  }), "interruptible");
  var uninterruptibleMask = /* @__PURE__ */ __name((f) => withFiber((fiber2) => {
    if (!fiber2.interruptible) return f(identity);
    fiber2.interruptible = false;
    fiber2._stack.push(setInterruptibleTrue);
    return f(interruptible);
  }), "uninterruptibleMask");
  var all = /* @__PURE__ */ __name((arg, options) => {
    if (isIterable(arg)) {
      return options?.mode === "result" ? forEach(arg, result, options) : forEach(arg, identity, options);
    } else if (options?.discard) {
      return options.mode === "result" ? forEach(Object.values(arg), result, options) : forEach(Object.values(arg), identity, options);
    }
    return suspend(() => {
      const out = {};
      return as(forEach(Object.entries(arg), ([key, effect2]) => map3(options?.mode === "result" ? result(effect2) : effect2, (value) => {
        assignProperty(out, key, value);
      }), {
        discard: true,
        concurrency: options?.concurrency
      }), out);
    });
  }, "all");
  var whileLoop = /* @__PURE__ */ makePrimitive({
    op: "While",
    [contA](value, fiber2) {
      this[args].step(value);
      if (this[args].while()) {
        fiber2._stack.push(this);
        return this[args].body();
      }
      return exitVoid;
    },
    [evaluate](fiber2) {
      if (this[args].while()) {
        fiber2._stack.push(this);
        return this[args].body();
      }
      return exitVoid;
    }
  });
  var forEach = /* @__PURE__ */ dual((args2) => typeof args2[1] === "function", (iterable, f, options) => suspend(() => {
    const concurrency = resolveConcurrency(options?.concurrency);
    if (concurrency === 1) {
      return forEachSequential(iterable, f, options);
    }
    const items = fromIterable(iterable);
    let length = items.length;
    if (length === 0) {
      return options?.discard ? void_ : succeed3([]);
    }
    const out = options?.discard ? void 0 : new Array(length);
    const eff = forEachConcurrent({
      f,
      out
    }, items, {
      concurrency
    });
    return eff ? as(eff, out) : succeed3(out);
  }));
  var forEachSequential = /* @__PURE__ */ __name((iterable, f, options) => suspend(() => {
    const out = options?.discard ? void 0 : [];
    const iterator = iterable[Symbol.iterator]();
    let state = iterator.next();
    let index = 0;
    return as(whileLoop({
      while: /* @__PURE__ */ __name(() => !state.done, "while"),
      body: /* @__PURE__ */ __name(() => f(state.value, index++), "body"),
      step: /* @__PURE__ */ __name((b) => {
        if (out) out.push(b);
        state = iterator.next();
      }, "step")
    }), out);
  }), "forEachSequential");
  var resolveConcurrency = /* @__PURE__ */ __name((concurrency) => concurrency === "unbounded" ? Number.POSITIVE_INFINITY : Math.max(1, concurrency ?? 1), "resolveConcurrency");
  var iterateEager = /* @__PURE__ */ __name(() => (options) => {
    const onItem = options.onItem;
    const step = options.step;
    const resumeSequential = /* @__PURE__ */ __name((state, items, index, end2, effect2) => flatMap2(exit(effect2), (itemExit) => step(state, items[index], itemExit, index) ?? runSequential(state, items, index + 1, end2) ?? void_), "resumeSequential");
    const runSequential = /* @__PURE__ */ __name((state, items, index = 0, end2 = items.length) => {
      for (; index < end2; index++) {
        const item = items[index];
        const effect2 = onItem(state, item, index);
        if (!effectIsExit(effect2)) {
          return resumeSequential(state, items, index, end2, effect2);
        }
        const terminal = step(state, item, effect2, index);
        if (terminal) return terminal._tag === "Failure" ? terminal : void 0;
      }
    }, "runSequential");
    return runSequential;
  }, "iterateEager");
  var iterateConcurrentImpl = /* @__PURE__ */ __name((options) => {
    const onItem = options.onItem;
    const step = options.step;
    return (state, items, opts) => {
      let index = 0;
      const end2 = opts.end ?? items.length;
      const concurrency = opts.concurrency;
      let done4 = false;
      let parentFiber;
      let fibers;
      let resume;
      let terminal;
      let effect2;
      const failDefect = /* @__PURE__ */ __name((error) => {
        const defect2 = exitDie(error);
        terminal = defect2;
        done4 = true;
        return fibers && fibers.size > 0 ? flatMap2(uninterruptible(fiberInterruptAll(Array.from(fibers))), () => terminal ?? defect2) : defect2;
      }, "failDefect");
      const go = /* @__PURE__ */ __name(() => {
        let paused = false;
        for (; !terminal && index < end2; index++) {
          const item = items[index];
          const eff = effect2 ?? onItem(state, item, index);
          if (effectIsExit(eff)) {
            terminal = step(state, item, eff, index);
            if (terminal) break;
          } else if (!parentFiber) {
            return callback((cb) => {
              parentFiber = getCurrentFiber();
              fibers = /* @__PURE__ */ new Set();
              effect2 = eff;
              resume = cb;
              let result2;
              try {
                result2 = go();
              } catch (error) {
                return cb(failDefect(error));
              }
              if (result2) return cb(result2);
              return suspend(() => {
                terminal ??= exitVoid;
                return flatMap2(fibers ? fiberInterruptAll(fibers) : void_, () => terminal?._tag === "Failure" ? terminal : void_);
              });
            });
          } else {
            effect2 = void 0;
            const fiber2 = forkUnsafe(parentFiber, eff, true, true, "inherit");
            if (fiber2._exit) {
              terminal = step(state, item, fiber2._exit, index);
              if (terminal) break;
              continue;
            }
            fibers.add(fiber2);
            const currentIndex = index;
            fiber2.addObserver((exit3) => {
              fibers.delete(fiber2);
              try {
                if (terminal) {
                  if (exit3._tag === "Failure") {
                    const reasons = exit3.cause.reasons.filter((reason) => reason._tag !== "Interrupt");
                    if (reasons.length > 0) {
                      const cause = causeFromReasons(reasons);
                      terminal = exitFailCause(terminal._tag === "Failure" ? causeCombine(terminal.cause, cause) : cause);
                    }
                  }
                } else {
                  const result2 = step(state, item, exit3, currentIndex);
                  if (result2) {
                    terminal = result2;
                    go();
                  }
                }
                if (paused) {
                  const eff2 = go();
                  if (eff2) resume(eff2);
                } else if (done4 && fibers.size === 0) {
                  resume(terminal ?? void_);
                }
              } catch (error) {
                resume(failDefect(error));
              }
            });
            if (fibers.size < concurrency) continue;
            paused = true;
            index++;
            return;
          }
        }
        done4 = true;
        if (terminal) {
          if (fibers && fibers.size > 0) {
            const annotations = fiberStackAnnotations(parentFiber);
            fibers.forEach((f) => f.interruptUnsafe(parentFiber.id, annotations));
            return;
          }
          if (resume || terminal._tag === "Failure") {
            return terminal;
          }
        } else if (resume) {
          if (!fibers) {
            return exitVoid;
          } else if (fibers.size === 0) {
            resume(void_);
          }
        }
      }, "go");
      return go();
    };
  }, "iterateConcurrentImpl");
  var iterateConcurrent = /* @__PURE__ */ __name(() => (options) => iterateConcurrentImpl(options), "iterateConcurrent");
  var forEachConcurrent = /* @__PURE__ */ iterateConcurrentImpl({
    onItem(state, item, index) {
      return state.f(item, index);
    },
    step(state, _, exit3, index) {
      if (exit3._tag === "Failure") return exit3;
      else if (state.out) {
        state.out[index] = exit3.value;
      }
    }
  });
  var filter = /* @__PURE__ */ dual((args2) => isIterable(args2[0]) && !isEffect(args2[0]), (elements, predicate, options) => suspend(() => {
    const out = [];
    return as(forEach(elements, (a, i) => {
      const result2 = predicate(a, i);
      if (typeof result2 === "boolean") {
        if (result2) out.push(a);
        return void_;
      }
      return map3(result2, (keep) => {
        if (keep) {
          out.push(a);
        }
      });
    }, {
      discard: true,
      concurrency: options?.concurrency
    }), out);
  }));
  var forkUnsafe = /* @__PURE__ */ __name((parent, effect2, immediate = false, daemon = false, uninterruptible3 = false) => {
    const parentRuntime = parent;
    const interruptible2 = uninterruptible3 === "inherit" ? parentRuntime.interruptible : !uninterruptible3;
    const child2 = new FiberImpl(parentRuntime.context, interruptible2);
    if (!daemon) {
      parentRuntime.children().add(child2);
      child2._parent = parentRuntime;
    }
    if (immediate) {
      child2.evaluate(effect2);
    } else {
      child2._asyncContext = captureAsyncContext();
      parentRuntime.currentDispatcher.scheduleTask(() => child2.evaluate(effect2), 0);
    }
    return child2;
  }, "forkUnsafe");
  var forkDetach = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self, options) => withFiber((fiber2) => succeed3(forkUnsafe(fiber2, self, options?.startImmediately, true, options?.uninterruptible))));
  var forkIn = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self, scope3, options) => withFiber((parent) => {
    const fiber2 = forkUnsafe(parent, self, options?.startImmediately, true, options?.uninterruptible);
    if (!fiber2._exit) {
      if (scope3.state._tag !== "Closed") {
        const key = {};
        const finalizer = /* @__PURE__ */ __name(() => withFiberId((interruptor) => interruptor === fiber2.id ? void_ : fiberInterrupt(fiber2)), "finalizer");
        scopeAddFinalizerUnsafe(scope3, key, finalizer);
        fiber2.addObserver(() => scopeRemoveFinalizerUnsafe(scope3, key));
      } else {
        fiber2.interruptUnsafe(parent.id, fiberStackAnnotations(parent));
      }
    }
    return succeed3(fiber2);
  }));
  var forkScoped = /* @__PURE__ */ dual((args2) => isEffect(args2[0]), (self, options) => flatMap2(scope, (scope3) => forkIn(self, scope3, options)));
  var runForkWith = /* @__PURE__ */ __name((context3) => (effect2, options) => {
    const fiber2 = new FiberImpl(options?.scheduler ? add(context3, Scheduler, options.scheduler) : context3, options?.uninterruptible !== true);
    fiber2.evaluate(effect2);
    if (fiber2._exit) return fiber2;
    if (options?.signal) {
      if (options.signal.aborted) {
        fiber2.interruptUnsafe();
      } else {
        const abort2 = /* @__PURE__ */ __name(() => fiber2.interruptUnsafe(), "abort");
        options.signal.addEventListener("abort", abort2, {
          once: true
        });
        fiber2.addObserver(() => options.signal.removeEventListener("abort", abort2));
      }
    }
    if (options?.onFiberStart) {
      options.onFiberStart(fiber2);
    }
    return fiber2;
  }, "runForkWith");
  var fiberRunIn = /* @__PURE__ */ dual(2, (self, scope3) => {
    if (self._exit) {
      return self;
    } else if (scope3.state._tag === "Closed") {
      self.interruptUnsafe(self.id);
      return self;
    }
    scopeRemoveFinalizerUnsafe(scope3, self);
    scopeAddFinalizerUnsafe(scope3, self, () => fiberInterrupt(self));
    self.addObserver(() => scopeRemoveFinalizerUnsafe(scope3, self));
    return self;
  });
  var runFork = /* @__PURE__ */ runForkWith(/* @__PURE__ */ empty());
  var runCallbackWith = /* @__PURE__ */ __name((context3) => {
    const runFork3 = runForkWith(context3);
    return (effect2, options) => {
      const fiber2 = runFork3(effect2, options);
      if (options?.onExit) {
        fiber2.addObserver(options.onExit);
      }
      return (interruptor) => {
        return fiber2.interruptUnsafe(interruptor);
      };
    };
  }, "runCallbackWith");
  var runCallback = /* @__PURE__ */ runCallbackWith(/* @__PURE__ */ empty());
  var runPromiseExitWith = /* @__PURE__ */ __name((context3) => {
    const runFork3 = runForkWith(context3);
    return (effect2, options) => {
      const fiber2 = runFork3(effect2, options);
      return new Promise((resolve3) => {
        fiber2.addObserver((exit3) => resolve3(exit3));
      });
    };
  }, "runPromiseExitWith");
  var runPromiseExit = /* @__PURE__ */ runPromiseExitWith(/* @__PURE__ */ empty());
  var runPromiseWith = /* @__PURE__ */ __name((context3) => {
    const runPromiseExit3 = runPromiseExitWith(context3);
    return (effect2, options) => runPromiseExit3(effect2, options).then((exit3) => {
      if (exit3._tag === "Failure") {
        throw causeSquash(exit3.cause);
      }
      return exit3.value;
    });
  }, "runPromiseWith");
  var runPromise = /* @__PURE__ */ runPromiseWith(/* @__PURE__ */ empty());
  var runSyncExitWith = /* @__PURE__ */ __name((context3) => {
    const runFork3 = runForkWith(context3);
    return (effect2) => {
      if (effectIsExit(effect2)) return effect2;
      const scheduler = new MixedScheduler("sync");
      const fiber2 = runFork3(effect2, {
        scheduler
      });
      fiber2._dispatcher?.flush();
      return fiber2._exit ?? exitDie(new AsyncFiberError(fiber2));
    };
  }, "runSyncExitWith");
  var runSyncExit = /* @__PURE__ */ runSyncExitWith(/* @__PURE__ */ empty());
  var runSyncWith = /* @__PURE__ */ __name((context3) => {
    const runSyncExit3 = runSyncExitWith(context3);
    return (effect2) => {
      const exit3 = runSyncExit3(effect2);
      if (exit3._tag === "Failure") throw causeSquash(exit3.cause);
      return exit3.value;
    };
  }, "runSyncWith");
  var runSync = /* @__PURE__ */ runSyncWith(/* @__PURE__ */ empty());
  var succeedTrue = /* @__PURE__ */ succeed3(true);
  var succeedFalse = /* @__PURE__ */ succeed3(false);
  var Latch = class {
    static {
      __name(this, "Latch");
    }
    waiters = [];
    scheduled = void 0;
    _isOpen;
    constructor(isOpen) {
      this._isOpen = isOpen;
    }
    scheduleUnsafe(fiber2) {
      if (this.waiters.length === 0) {
        return succeedTrue;
      }
      if (this.scheduled === void 0) {
        this.scheduled = this.waiters;
        fiber2.currentDispatcher.scheduleTask(this.flushScheduled, 0);
      } else {
        for (let i = 0; i < this.waiters.length; i++) {
          this.scheduled.push(this.waiters[i]);
        }
      }
      this.waiters = [];
      return succeedTrue;
    }
    flushScheduled = /* @__PURE__ */ __name(() => {
      if (this.scheduled === void 0) return;
      const waiters = this.scheduled;
      this.scheduled = void 0;
      for (let i = 0; i < waiters.length; i++) {
        waiters[i](exitVoid);
      }
    }, "flushScheduled");
    flushWaiters() {
      const waiters = this.waiters;
      this.waiters = [];
      this.flushScheduled();
      for (let i = 0; i < waiters.length; i++) {
        waiters[i](exitVoid);
      }
    }
    open = /* @__PURE__ */ withFiber((fiber2) => {
      if (this._isOpen) return succeedFalse;
      this._isOpen = true;
      return this.scheduleUnsafe(fiber2);
    });
    release = /* @__PURE__ */ withFiber((fiber2) => this._isOpen ? succeedFalse : this.scheduleUnsafe(fiber2));
    openUnsafe() {
      if (this._isOpen) return false;
      this._isOpen = true;
      this.flushWaiters();
      return true;
    }
    await = /* @__PURE__ */ callback((resume) => {
      if (this._isOpen) {
        return resume(void_);
      }
      this.waiters.push(resume);
      return sync(() => {
        let index = this.waiters.indexOf(resume);
        if (index !== -1) {
          this.waiters.splice(index, 1);
        } else if (this.scheduled !== void 0) {
          index = this.scheduled.indexOf(resume);
          if (index !== -1) {
            this.scheduled.splice(index, 1);
          }
        }
      });
    });
    closeUnsafe() {
      if (!this._isOpen) return false;
      this._isOpen = false;
      return true;
    }
    close = /* @__PURE__ */ sync(() => this.closeUnsafe());
    whenOpen = /* @__PURE__ */ __name((self) => flatMap2(this.await, () => self), "whenOpen");
    isOpen() {
      return this._isOpen;
    }
  };
  var makeLatchUnsafe = /* @__PURE__ */ __name((open) => new Latch(open ?? false), "makeLatchUnsafe");
  var bigint02 = /* @__PURE__ */ BigInt(0);
  var NoopSpanProto = {
    _tag: "Span",
    spanId: "noop",
    traceId: "noop",
    sampled: false,
    status: {
      _tag: "Ended",
      startTime: bigint02,
      endTime: bigint02,
      exit: exitVoid
    },
    attributes: /* @__PURE__ */ new Map(),
    links: [],
    kind: "internal",
    attribute() {
    },
    event() {
    },
    end() {
    },
    addLinks() {
    }
  };
  var noopSpan = /* @__PURE__ */ __name((options) => Object.assign(Object.create(NoopSpanProto), options), "noopSpan");
  var filterDisablePropagation = /* @__PURE__ */ __name((span) => {
    if (!span) return none2();
    return get(span.annotations, DisablePropagation) ? span._tag === "Span" ? filterDisablePropagation(getOrUndefined(span.parent)) : none2() : some2(span);
  }, "filterDisablePropagation");
  var makeSpanUnsafe = /* @__PURE__ */ __name((fiber2, name, options) => {
    const disablePropagation = !fiber2.cache.tracerEnabled || options?.annotations && get(options.annotations, DisablePropagation);
    const parent = options?.parent !== void 0 ? some2(options.parent) : options?.root ? none2() : filterDisablePropagation(fiber2.cache.span);
    let span;
    if (disablePropagation) {
      span = noopSpan({
        name,
        parent,
        annotations: add(options?.annotations ?? empty(), DisablePropagation, true)
      });
    } else {
      const tracer2 = fiber2.cache.tracer ?? nativeTracer;
      const clock = fiber2.getRef(ClockRef);
      const timingEnabled = fiber2.getRef(TracerTimingEnabled);
      const annotationsFromEnv = fiber2.getRef(TracerSpanAnnotations);
      const linksFromEnv = fiber2.getRef(TracerSpanLinks);
      const level = options?.level ?? fiber2.getRef(CurrentTraceLevel);
      const links = options?.links !== void 0 ? [...linksFromEnv, ...options.links] : linksFromEnv.length === 0 ? [] : linksFromEnv.slice();
      span = tracer2.span({
        name,
        parent,
        annotations: options?.annotations ?? empty(),
        links,
        startTime: timingEnabled ? clock.currentTimeNanosUnsafe() : bigint02,
        kind: options?.kind ?? "internal",
        root: options?.root ?? false,
        sampled: options?.sampled ?? (isSome2(parent) && parent.value.sampled === false ? false : !isLogLevelGreaterThan(fiber2.getRef(MinimumTraceLevel), level))
      });
      for (const key in annotationsFromEnv) {
        span.attribute(key, annotationsFromEnv[key]);
      }
      if (options?.attributes !== void 0) {
        for (const key in options.attributes) {
          span.attribute(key, options.attributes[key]);
        }
      }
    }
    return span;
  }, "makeSpanUnsafe");
  var endSpan = /* @__PURE__ */ __name((span, exit3, clock, timingEnabled) => sync(() => {
    if (span.status._tag === "Ended") return;
    span.end(timingEnabled ? clock.currentTimeNanosUnsafe() : bigint02, exit3);
  }), "endSpan");
  var useSpan = /* @__PURE__ */ __name((name, ...args2) => {
    const options = args2.length === 1 ? void 0 : args2[0];
    const evaluate2 = args2[args2.length - 1];
    return withFiber((fiber2) => {
      const span = makeSpanUnsafe(fiber2, name, options);
      const clock = fiber2.getRef(ClockRef);
      const timingEnabled = fiber2.getRef(TracerTimingEnabled);
      onExitUnsafe(fiber2, (exit3) => endSpan(span, exit3, clock, timingEnabled));
      return evaluate2(span);
    });
  }, "useSpan");
  var provideParentSpan = /* @__PURE__ */ provideService(ParentSpan);
  var ClockRef = /* @__PURE__ */ Reference("effect/Clock", {
    defaultValue: /* @__PURE__ */ __name(() => new ClockImpl(), "defaultValue")
  });
  var MAX_TIMER_MILLIS = 2 ** 31 - 1;
  var ClockImpl = class {
    static {
      __name(this, "ClockImpl");
    }
    currentTimeMillisUnsafe() {
      return Date.now();
    }
    currentTimeMillis = /* @__PURE__ */ sync(() => this.currentTimeMillisUnsafe());
    currentTimeNanosUnsafe() {
      return wallTimeNanos();
    }
    currentTimeNanos = /* @__PURE__ */ sync(() => this.currentTimeNanosUnsafe());
    monotonicTimeNanosUnsafe() {
      return monotonicNowNanos();
    }
    monotonicTimeNanos = /* @__PURE__ */ sync(() => this.monotonicTimeNanosUnsafe());
    sleep(duration) {
      return this.sleepMillis(toMillis(duration));
    }
    sleepMillis(millis2) {
      if (millis2 <= 0) return yieldNow;
      else if (!Number.isFinite(millis2)) return never;
      return callback((resume) => {
        const continuation = millis2 > MAX_TIMER_MILLIS ? this.sleepMillis(millis2 - MAX_TIMER_MILLIS) : void_;
        const handle = setTimeout(() => resume(continuation), Math.min(millis2, MAX_TIMER_MILLIS));
        return sync(() => clearTimeout(handle));
      });
    }
  };
  var nanosPerMilli = /* @__PURE__ */ BigInt(1e6);
  var monotonicNowNanos = /* @__PURE__ */ (function() {
    const processHrtime = globalThis.process?.hrtime;
    if (typeof processHrtime?.bigint === "function") {
      return () => processHrtime.bigint();
    }
    if (typeof performance !== "undefined" && typeof performance.now === "function") {
      return () => BigInt(Math.round(performance.now() * 1e6));
    }
    let previous = /* @__PURE__ */ BigInt(0);
    return () => {
      const current = BigInt(Date.now()) * nanosPerMilli;
      if (current > previous) {
        previous = current;
      }
      return previous;
    };
  })();
  var wallTimeNanos = /* @__PURE__ */ (function() {
    const reanchorThresholdNanos = /* @__PURE__ */ BigInt(1e9);
    let origin;
    return () => {
      const monotonic = monotonicNowNanos();
      const wall = BigInt(Date.now()) * nanosPerMilli;
      if (origin === void 0) {
        origin = wall - monotonic;
      } else {
        const projected = origin + monotonic;
        const skew = wall > projected ? wall - projected : projected - wall;
        if (skew > reanchorThresholdNanos) {
          origin = wall - monotonic;
        }
      }
      return origin + monotonic;
    };
  })();
  var clockWith = /* @__PURE__ */ __name((f) => withFiber((fiber2) => f(fiber2.getRef(ClockRef))), "clockWith");
  var sleep = /* @__PURE__ */ __name((duration) => clockWith((clock) => clock.sleep(fromInputUnsafe(duration))), "sleep");
  var currentTimeMillis = /* @__PURE__ */ clockWith((clock) => clock.currentTimeMillis);
  var TimeoutErrorTypeId = "~effect/Cause/TimeoutError";
  var TimeoutError = class extends (/* @__PURE__ */ TaggedError("TimeoutError")) {
    static {
      __name(this, "TimeoutError");
    }
    [TimeoutErrorTypeId] = TimeoutErrorTypeId;
    constructor(message2) {
      super({
        message: message2
      });
    }
  };
  var IllegalArgumentErrorTypeId = "~effect/Cause/IllegalArgumentError";
  var IllegalArgumentError = class extends (/* @__PURE__ */ TaggedError("IllegalArgumentError")) {
    static {
      __name(this, "IllegalArgumentError");
    }
    [IllegalArgumentErrorTypeId] = IllegalArgumentErrorTypeId;
    constructor(message2) {
      super({
        message: message2
      });
    }
  };
  var ExceededCapacityErrorTypeId = "~effect/Cause/ExceededCapacityError";
  var ExceededCapacityError = class extends (/* @__PURE__ */ TaggedError("ExceededCapacityError")) {
    static {
      __name(this, "ExceededCapacityError");
    }
    [ExceededCapacityErrorTypeId] = ExceededCapacityErrorTypeId;
    constructor(message2) {
      super({
        message: message2
      });
    }
  };
  var AsyncFiberErrorTypeId = "~effect/Cause/AsyncFiberError";
  var AsyncFiberError = class extends (/* @__PURE__ */ TaggedError("AsyncFiberError")) {
    static {
      __name(this, "AsyncFiberError");
    }
    [AsyncFiberErrorTypeId] = AsyncFiberErrorTypeId;
    constructor(fiber2) {
      super({
        message: "An asynchronous Effect was executed with Effect.runSync",
        fiber: fiber2
      });
    }
  };
  var UnknownErrorTypeId = "~effect/Cause/UnknownError";
  var UnknownError = class extends (/* @__PURE__ */ TaggedError("UnknownError")) {
    static {
      __name(this, "UnknownError");
    }
    [UnknownErrorTypeId] = UnknownErrorTypeId;
    constructor(cause, message2) {
      super({
        message: message2,
        cause
      });
    }
  };
  var ConsoleRef = /* @__PURE__ */ Reference("effect/Console", {
    defaultValue: /* @__PURE__ */ __name(() => globalThis.console, "defaultValue")
  });
  var logLevelToOrder = /* @__PURE__ */ __name((level) => {
    switch (level) {
      case "All":
        return Number.MIN_SAFE_INTEGER;
      case "Fatal":
        return 5e4;
      case "Error":
        return 4e4;
      case "Warn":
        return 3e4;
      case "Info":
        return 2e4;
      case "Debug":
        return 1e4;
      case "Trace":
        return 0;
      case "None":
        return Number.MAX_SAFE_INTEGER;
    }
  }, "logLevelToOrder");
  var LogLevelOrder = /* @__PURE__ */ mapInput(Number2, logLevelToOrder);
  var isLogLevelGreaterThan = /* @__PURE__ */ isGreaterThan(LogLevelOrder);
  var CurrentLoggers = /* @__PURE__ */ Reference("effect/Logger/CurrentLoggers", {
    defaultValue: /* @__PURE__ */ __name(() => /* @__PURE__ */ new Set([defaultLogger, tracerLogger]), "defaultValue")
  });
  var LogToStderr = /* @__PURE__ */ Reference("effect/Logger/LogToStderr", {
    defaultValue: constFalse
  });
  var LoggerTypeId = "~effect/Logger";
  var LoggerProto = {
    [LoggerTypeId]: {
      _Message: identity,
      _Output: identity
    },
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var loggerMake = /* @__PURE__ */ __name((log2) => {
    const self = Object.create(LoggerProto);
    self.log = log2;
    return self;
  }, "loggerMake");
  var formatLabel = /* @__PURE__ */ __name((key) => key.replace(/[\s="]/g, "_"), "formatLabel");
  var formatLogSpan = /* @__PURE__ */ __name((self, now) => {
    const label = formatLabel(self[0]);
    return `${label}=${now - self[1]}ms`;
  }, "formatLogSpan");
  var logWithLevel = /* @__PURE__ */ __name((level) => (...message2) => {
    let cause = void 0;
    for (let i = 0, len = message2.length; i < len; i++) {
      const msg = message2[i];
      if (isCause(msg)) {
        if (cause) {
          ;
          message2.splice(i, 1);
        } else {
          message2 = message2.slice(0, i).concat(message2.slice(i + 1));
        }
        cause = cause ? causeFromReasons(cause.reasons.concat(msg.reasons)) : msg;
        i--;
      }
    }
    if (cause === void 0) {
      cause = causeEmpty;
    }
    return withFiber((fiber2) => {
      const logLevel = level ?? fiber2.cache.logLevel;
      if (isLogLevelGreaterThan(fiber2.cache.minimumLogLevel, logLevel)) {
        return void_;
      }
      const clock = fiber2.getRef(ClockRef);
      const loggers = fiber2.getRef(CurrentLoggers);
      if (loggers.size > 0) {
        const date2 = new Date(clock.currentTimeMillisUnsafe());
        for (const logger of loggers) {
          logger.log({
            cause,
            fiber: fiber2,
            date: date2,
            logLevel,
            message: message2
          });
        }
      }
      return void_;
    });
  }, "logWithLevel");
  var colors = {
    bold: "1",
    red: "31",
    green: "32",
    yellow: "33",
    blue: "34",
    cyan: "36",
    white: "37",
    gray: "90",
    black: "30",
    bgBrightRed: "101"
  };
  var logLevelColors = {
    None: [],
    All: [],
    Trace: [colors.gray],
    Debug: [colors.blue],
    Info: [colors.green],
    Warn: [colors.yellow],
    Error: [colors.red],
    Fatal: [colors.bgBrightRed, colors.black]
  };
  var defaultDateFormat = /* @__PURE__ */ __name((date2) => `${date2.getHours().toString().padStart(2, "0")}:${date2.getMinutes().toString().padStart(2, "0")}:${date2.getSeconds().toString().padStart(2, "0")}.${date2.getMilliseconds().toString().padStart(3, "0")}`, "defaultDateFormat");
  var defaultLogger = /* @__PURE__ */ loggerMake(({
    cause,
    date: date2,
    fiber: fiber2,
    logLevel,
    message: message2
  }) => {
    const message_ = Array.isArray(message2) ? message2.slice() : [message2];
    if (cause.reasons.length > 0) {
      message_.push(causePretty(cause));
    }
    const now = date2.getTime();
    const spans = fiber2.getRef(CurrentLogSpans);
    let spanString = "";
    for (const span of spans) {
      spanString += ` ${formatLogSpan(span, now)}`;
    }
    const annotations = fiber2.getRef(CurrentLogAnnotations);
    if (Object.keys(annotations).length > 0) {
      message_.push(annotations);
    }
    const console2 = fiber2.getRef(ConsoleRef);
    const log2 = fiber2.getRef(LogToStderr) ? console2.error : console2.log;
    log2(`[${defaultDateFormat(date2)}] ${logLevel.toUpperCase()} (#${fiber2.id})${spanString}:`, ...message_);
  });
  var tracerLogger = /* @__PURE__ */ loggerMake(({
    cause,
    fiber: fiber2,
    logLevel,
    message: message2
  }) => {
    const clock = fiber2.getRef(ClockRef);
    const annotations = fiber2.getRef(CurrentLogAnnotations);
    const span = fiber2.cache.span;
    if (span === void 0 || span._tag === "ExternalSpan") return;
    const attributes = {};
    for (const [key, value] of Object.entries(annotations)) {
      assignProperty(attributes, key, value);
    }
    attributes["effect.fiberId"] = fiber2.id;
    attributes["effect.logLevel"] = logLevel.toUpperCase();
    if (cause.reasons.length > 0) {
      attributes["effect.cause"] = causePretty(cause);
    }
    span.event(toStringUnknown(Array.isArray(message2) && message2.length === 1 ? message2[0] : message2), clock.currentTimeNanosUnsafe(), attributes);
  });

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Exit.js
  var succeed4 = exitSucceed;
  var failCause2 = exitFailCause;
  var fail4 = exitFail;
  var interrupt2 = exitInterrupt;
  var void_2 = exitVoid;
  var isSuccess3 = exitIsSuccess;
  var isFailure3 = exitIsFailure;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Deferred.js
  var TypeId5 = "~effect/Deferred";
  var DeferredProto = {
    [TypeId5]: {
      _A: identity,
      _E: identity
    },
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var DeferredImpl = /* @__PURE__ */ __name(function() {
    this.resumes = void 0;
    this.effect = void 0;
  }, "DeferredImpl");
  DeferredImpl.prototype = DeferredProto;
  var makeUnsafe2 = /* @__PURE__ */ __name(() => new DeferredImpl(), "makeUnsafe");
  var make5 = /* @__PURE__ */ __name(() => sync(() => makeUnsafe2()), "make");
  var _await = /* @__PURE__ */ __name((self) => callback((resume) => {
    if (self.effect) return resume(self.effect);
    self.resumes ??= [];
    self.resumes.push(resume);
    return sync(() => {
      const resumes = self.resumes;
      if (resumes === void 0) return;
      const index = resumes.indexOf(resume);
      if (index >= 0) resumes.splice(index, 1);
    });
  }), "_await");
  var completeWith = /* @__PURE__ */ dual(2, (self, effect2) => sync(() => doneUnsafe(self, effect2)));
  var done2 = completeWith;
  var failCause3 = /* @__PURE__ */ dual(2, (self, cause) => done2(self, exitFailCause(cause)));
  var interruptWith = /* @__PURE__ */ dual(2, (self, fiberId3) => failCause3(self, causeInterrupt(fiberId3)));
  var isDone2 = /* @__PURE__ */ __name((self) => sync(() => isDoneUnsafe(self)), "isDone");
  var isDoneUnsafe = /* @__PURE__ */ __name((self) => self.effect !== void 0, "isDoneUnsafe");
  var succeed5 = /* @__PURE__ */ dual(2, (self, value) => done2(self, exitSucceed(value)));
  var doneUnsafe = /* @__PURE__ */ __name((self, effect2) => {
    if (self.effect) return false;
    self.effect = effect2;
    if (self.resumes) {
      const resumes = self.resumes;
      self.resumes = void 0;
      for (let i = 0; i < resumes.length; i++) {
        resumes[i](effect2);
      }
    }
    return true;
  }, "doneUnsafe");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/References.js
  var CurrentLogAnnotations2 = CurrentLogAnnotations;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Scope.js
  var Scope = scopeTag;
  var make6 = scopeMake;
  var makeUnsafe3 = scopeMakeUnsafe;
  var provide = provideScope;
  var addFinalizerExit = scopeAddFinalizerExit;
  var addFinalizer2 = scopeAddFinalizer;
  var forkUnsafe2 = scopeForkUnsafe;
  var close = scopeClose;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Layer.js
  var TypeId6 = "~effect/Layer";
  var MemoMapTypeId = "~effect/Layer/MemoMap";
  var makeMemoMapEntry = /* @__PURE__ */ __name((memoMap, layer8) => {
    const entry = {
      observers: 0,
      deferred: makeUnsafe2(),
      scope: makeUnsafe3(),
      finalizer: /* @__PURE__ */ __name((exit3) => suspend(() => {
        if (--entry.observers > 0) return void_;
        memoMap.map.delete(layer8);
        return close(entry.scope, exit3);
      }), "finalizer")
    };
    return entry;
  }, "makeMemoMapEntry");
  var memoMapObserve = /* @__PURE__ */ __name((entry, scope3) => {
    if (scope3.state._tag === "Closed") return false;
    entry.observers++;
    scopeAddFinalizerUnsafe(scope3, {}, entry.finalizer);
    return true;
  }, "memoMapObserve");
  var LayerProto = {
    [TypeId6]: {
      _ROut: identity,
      _E: identity,
      _RIn: identity
    },
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var fromBuildUnsafe = /* @__PURE__ */ __name((build) => {
    const self = Object.create(LayerProto);
    self.build = build;
    return self;
  }, "fromBuildUnsafe");
  var fromBuild = /* @__PURE__ */ __name((build) => fromBuildUnsafe((memoMap, scope3) => {
    const layerScope = forkUnsafe2(scope3);
    return onExit(build(memoMap, layerScope), (exit3) => exit3._tag === "Failure" ? close(layerScope, exit3) : void_);
  }), "fromBuild");
  var fromBuildMemo = /* @__PURE__ */ __name((build) => {
    const self = fromBuild((memoMap, scope3) => memoMap.getOrElseMemoize(self, scope3, build));
    return self;
  }, "fromBuildMemo");
  var MemoMapImpl = class {
    static {
      __name(this, "MemoMapImpl");
    }
    get [MemoMapTypeId]() {
      return MemoMapTypeId;
    }
    parent;
    constructor(parent) {
      this.parent = parent;
    }
    map = /* @__PURE__ */ new Map();
    get(layer8, scope3) {
      const local = this.map.get(layer8);
      if (local) {
        memoMapObserve(local, scope3);
        return local.deferred.effect ?? _await(local.deferred);
      }
      return this.parent?.get(layer8, scope3);
    }
    getOrElseMemoize(layer8, scope3, build) {
      return suspend(() => {
        let deferred;
        return onExitPrimitive(suspend(() => {
          const existing = this.get(layer8, scope3);
          if (existing) return existing;
          const entry = makeMemoMapEntry(this, layer8);
          if (!memoMapObserve(entry, scope3)) return build(this, scope3);
          deferred = entry.deferred;
          this.map.set(layer8, entry);
          return build(this, entry.scope);
        }), (exit3) => {
          if (deferred) doneUnsafe(deferred, exit3);
        });
      });
    }
  };
  var makeMemoMapUnsafe = /* @__PURE__ */ __name(() => new MemoMapImpl(), "makeMemoMapUnsafe");
  var forkMemoMapUnsafe = /* @__PURE__ */ __name((parent) => new MemoMapImpl(parent), "forkMemoMapUnsafe");
  var CurrentMemoMap = class _CurrentMemoMap extends (/* @__PURE__ */ Service()("effect/Layer/CurrentMemoMap")) {
    static {
      __name(this, "CurrentMemoMap");
    }
    static forkOrCreate(self) {
      const current = getOrUndefined2(self, _CurrentMemoMap);
      return current ? forkMemoMapUnsafe(current) : makeMemoMapUnsafe();
    }
  };
  var buildWithMemoMap = /* @__PURE__ */ dual(3, (self, memoMap, scope3) => provideService(map3(suspend(() => self.build(memoMap, scope3)), add(CurrentMemoMap, memoMap)), CurrentMemoMap, memoMap));
  var succeed6 = /* @__PURE__ */ __name(function() {
    if (arguments.length === 1) {
      return (resource) => succeedContext(make2(arguments[0], resource));
    }
    return succeedContext(make2(arguments[0], arguments[1]));
  }, "succeed");
  var succeedContext = /* @__PURE__ */ __name((context3) => fromBuildUnsafe(constant(succeed3(context3))), "succeedContext");
  var effect = /* @__PURE__ */ __name(function() {
    if (arguments.length === 1) {
      return (effect2) => effectImpl(arguments[0], effect2);
    }
    return effectImpl(arguments[0], arguments[1]);
  }, "effect");
  var effectImpl = /* @__PURE__ */ __name((service3, effect2) => effectContext(map3(effect2, (value) => make2(service3, value))), "effectImpl");
  var effectContext = /* @__PURE__ */ __name((effect2) => fromBuildMemo((_, scope3) => provide(effect2, scope3)), "effectContext");
  var effectDiscard = /* @__PURE__ */ __name((effect2) => effectContext(as(effect2, empty())), "effectDiscard");
  var mergeAllEffect = /* @__PURE__ */ __name((layers, memoMap, scope3) => {
    const parentScope = forkUnsafe2(scope3, "parallel");
    return forEach(layers, (layer8) => layer8.build(memoMap, forkUnsafe2(parentScope, "sequential")), {
      concurrency: layers.length
    }).pipe(map3((context3) => mergeAll(...context3)));
  }, "mergeAllEffect");
  var mergeAll2 = /* @__PURE__ */ __name((...layers) => fromBuild((memoMap, scope3) => mergeAllEffect(layers, memoMap, scope3)), "mergeAll");
  var provideWith = /* @__PURE__ */ __name((self, that, f) => fromBuild((memoMap, scope3) => flatMap2(Array.isArray(that) ? mergeAllEffect(that, memoMap, scope3) : that.build(memoMap, scope3), (context3) => self.build(memoMap, scope3).pipe(provideContext(context3), map3((merged) => f(merged, context3))))), "provideWith");
  var provide2 = /* @__PURE__ */ dual(2, (self, that) => provideWith(self, that, identity));
  var provideMerge = /* @__PURE__ */ dual(2, (self, that) => provideWith(self, that, (self2, that2) => merge(that2, self2)));

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Cause.js
  var isFailReason2 = isFailReason;
  var isInterruptReason2 = isInterruptReason;
  var fromReasons = causeFromReasons;
  var die2 = causeDie;
  var hasInterruptsOnly2 = hasInterruptsOnly;
  var map4 = causeMap;
  var squash = causeSquash;
  var hasDies2 = hasDies;
  var pretty = causePretty;
  var isDone3 = isDone;
  var done3 = done;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Clock.js
  var currentTimeMillis2 = currentTimeMillis;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Number.js
  var Number3 = globalThis.Number;
  var remainder = /* @__PURE__ */ dual(2, (self, divisor) => {
    if (!globalThis.Number.isFinite(self) || !globalThis.Number.isFinite(divisor) || divisor === 0) {
      return NaN;
    }
    if (globalThis.Number.isInteger(self) && globalThis.Number.isInteger(divisor)) {
      return self % divisor;
    }
    const [selfCoefficient, selfExponent] = toScientificInteger(self);
    const [divisorCoefficient, divisorExponent] = toScientificInteger(divisor);
    const exponent = Math.min(selfExponent, divisorExponent);
    const selfInteger = selfCoefficient * BigInt(10) ** BigInt(selfExponent - exponent);
    const divisorInteger = divisorCoefficient * BigInt(10) ** BigInt(divisorExponent - exponent);
    const out = selfInteger % divisorInteger;
    if (out === BigInt(0)) {
      return self < 0 || Object.is(self, -0) ? -0 : 0;
    }
    const remainder2 = globalThis.Number(`${out}e${exponent}`);
    return remainder2 === 0 ? Math.sign(self) * globalThis.Number.MIN_VALUE : remainder2;
  });
  function toScientificInteger(n) {
    if (globalThis.Number.isInteger(n)) {
      return [BigInt(n), 0];
    }
    const scientific = Math.abs(n).toExponential();
    const eIndex = scientific.indexOf("e");
    const digits = scientific.slice(0, eIndex).replace(".", "");
    const coefficient = BigInt(digits) * (n < 0 ? -BigInt(1) : BigInt(1));
    return [coefficient, globalThis.Number(scientific.slice(eIndex + 1)) - digits.length + 1];
  }
  __name(toScientificInteger, "toScientificInteger");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/random.js
  var Random = /* @__PURE__ */ Reference("effect/Random", {
    defaultValue: /* @__PURE__ */ __name(() => ({
      nextIntUnsafe() {
        return Math.floor(Math.random() * (Number.MAX_SAFE_INTEGER - Number.MIN_SAFE_INTEGER + 1)) + Number.MIN_SAFE_INTEGER;
      },
      nextDoubleUnsafe() {
        return Math.random();
      }
    }), "defaultValue")
  });

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Pull.js
  var catchDone = /* @__PURE__ */ dual(2, (effect2, f) => catchCauseFilter(effect2, filterDoneLeftover, (l) => f(l)));
  var isDoneCause = /* @__PURE__ */ __name((cause) => cause.reasons.some(isDoneFailure), "isDoneCause");
  var isDoneFailure = /* @__PURE__ */ __name((failure2) => failure2._tag === "Fail" && isDone3(failure2.error), "isDoneFailure");
  var filterDone = /* @__PURE__ */ __name((cause) => {
    let done4;
    let hasFailure = false;
    for (const reason of cause.reasons) {
      if (isDoneFailure(reason)) {
        done4 ??= reason.error;
      } else if (reason._tag !== "Interrupt") {
        hasFailure = true;
      }
    }
    if (done4 === void 0) return fail2(cause);
    return hasFailure ? fail2(fromReasons(cause.reasons.filter((reason) => !isDoneFailure(reason)))) : succeed2(done4);
  }, "filterDone");
  var filterDoneLeftover = /* @__PURE__ */ __name((cause) => {
    const done4 = filterDone(cause);
    return isFailure2(done4) ? done4 : succeed2(done4.success.value);
  }, "filterDoneLeftover");
  var doneExitFromCause = /* @__PURE__ */ __name((cause) => {
    const halt = filterDone(cause);
    return !isFailure2(halt) ? succeed4(halt.success.value) : failCause2(halt.failure);
  }, "doneExitFromCause");
  var matchEffect = /* @__PURE__ */ dual(2, (self, options) => matchCauseEffect(self, {
    onSuccess: options.onSuccess,
    onFailure: /* @__PURE__ */ __name((cause) => {
      const halt = filterDone(cause);
      return !isFailure2(halt) ? options.onDone(halt.success.value) : options.onFailure(halt.failure);
    }, "onFailure")
  }));

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Schedule.js
  var TypeId7 = "~effect/Schedule";
  var randomNext = /* @__PURE__ */ Random.useSync((random3) => random3.nextDoubleUnsafe());
  var CurrentMetadata = /* @__PURE__ */ Reference("effect/Schedule/CurrentMetadata", {
    defaultValue: /* @__PURE__ */ constant({
      input: void 0,
      output: void 0,
      duration: zero,
      attempt: 0,
      start: 0,
      now: 0,
      elapsed: 0,
      elapsedSincePrevious: 0
    })
  });
  var ScheduleProto = {
    [TypeId7]: {
      _Out: identity,
      _In: identity,
      _Env: identity
    },
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var isSchedule = /* @__PURE__ */ __name((u) => hasProperty(u, TypeId7), "isSchedule");
  var fromStep = /* @__PURE__ */ __name((step) => {
    const self = Object.create(ScheduleProto);
    self.step = step;
    return self;
  }, "fromStep");
  var metadataFn = /* @__PURE__ */ __name(() => {
    let n = 0;
    let previous;
    let start3;
    return (now, input) => {
      if (start3 === void 0) start3 = now;
      const elapsed = now - start3;
      const elapsedSincePrevious = previous === void 0 ? 0 : now - previous;
      previous = now;
      return {
        input,
        attempt: ++n,
        start: start3,
        now,
        elapsed,
        elapsedSincePrevious
      };
    };
  }, "metadataFn");
  var fromStepWithMetadata = /* @__PURE__ */ __name((step) => fromStep(map3(step, (f) => {
    const meta = metadataFn();
    return (now, input) => f(meta(now, input));
  })), "fromStepWithMetadata");
  var toStep = /* @__PURE__ */ __name((schedule4) => catchCause(schedule4.step, (cause) => succeed3(() => failCause(cause))), "toStep");
  var toStepWithMetadata = /* @__PURE__ */ __name((schedule4) => clockWith((clock) => map3(toStep(schedule4), (step) => {
    const metaFn = metadataFn();
    return (input) => suspend(() => {
      const now = clock.currentTimeMillisUnsafe();
      return flatMap2(step(now, input), ([output, duration]) => {
        const meta = metaFn(now, input);
        meta.output = output;
        meta.duration = duration;
        return as(sleep(duration), meta);
      });
    });
  })), "toStepWithMetadata");
  var toStepWithSleep = /* @__PURE__ */ __name((schedule4) => map3(toStepWithMetadata(schedule4), (step) => (input) => map3(step(input), (meta) => meta.output)), "toStepWithSleep");
  var exponential = /* @__PURE__ */ __name((base, factor = 2) => {
    const baseMillis = toMillis(fromInputUnsafe(base));
    return fromStepWithMetadata(succeed3((meta) => {
      const duration = millis(baseMillis * Math.pow(factor, meta.attempt - 1));
      return succeed3([duration, duration]);
    }));
  }, "exponential");
  var modifyDelay = /* @__PURE__ */ dual(2, (self, f) => fromStep(map3(toStep(self), (step) => {
    const meta = metadataFn();
    return (now, input) => flatMap2(step(now, input), ([output, duration]) => map3(f({
      ...meta(now, input),
      output,
      duration
    }), (replacement) => [output, fromInputUnsafe(replacement)]));
  })));
  var jittered = /* @__PURE__ */ __name((self) => modifyDelay(self, ({
    duration
  }) => map3(randomNext, (random3) => {
    const millis2 = toMillis(duration);
    return millis(millis2 * 0.8 * (1 - random3) + millis2 * 1.2 * random3);
  })), "jittered");
  var passthrough = /* @__PURE__ */ __name((self) => fromStep(map3(toStep(self), (step) => (now, input) => matchEffect(step(now, input), {
    onSuccess: /* @__PURE__ */ __name((result2) => succeed3([input, result2[1]]), "onSuccess"),
    onFailure: failCause,
    onDone: /* @__PURE__ */ __name(() => done3(input), "onDone")
  }))), "passthrough");
  var spaced = /* @__PURE__ */ __name((duration) => {
    const decoded = fromInputUnsafe(duration);
    return fromStepWithMetadata(succeed3((meta) => succeed3([meta.attempt - 1, decoded])));
  }, "spaced");
  var upTo = /* @__PURE__ */ dual(2, (self, options) => {
    const duration = options.duration === void 0 ? void 0 : fromInputUnsafe(options.duration);
    return while_(self, ({
      attempt,
      elapsed
    }) => succeed3((options.times === void 0 || attempt <= options.times) && (duration === void 0 || isLessThanOrEqualTo2(millis(elapsed), duration))));
  });
  var while_ = /* @__PURE__ */ dual(2, (self, predicate) => fromStep(map3(toStep(self), (step) => {
    const meta = metadataFn();
    return (now, input) => flatMap2(step(now, input), (result2) => {
      const [output, duration] = result2;
      const eff = predicate({
        ...meta(now, input),
        output,
        duration
      });
      return flatMap2(isEffect(eff) ? eff : succeed3(eff), (check) => check ? succeed3(result2) : done3(output));
    });
  })));
  var forever2 = /* @__PURE__ */ spaced(zero);

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schedule.js
  var findErrorOnly = /* @__PURE__ */ __name((cause) => cause.reasons.every(isFailReason) ? findError(cause) : fail2(cause), "findErrorOnly");
  var repeatOrElse = /* @__PURE__ */ dual(3, (self, schedule4, orElse) => flatMap2(toStepWithMetadata(schedule4), (step) => {
    let meta = CurrentMetadata.defaultValue();
    return catchCauseFilter(catchDone(forever(tap(flatMap2(suspend(() => provideService(self, CurrentMetadata, meta)), step), (meta_) => sync(() => {
      meta = meta_;
    })), {
      disableYield: true
    }), (out) => succeed3(out)), findErrorOnly, (error) => orElse(error, meta.attempt === 0 ? none2() : some2(meta)));
  }));
  var retryOrElse = /* @__PURE__ */ dual(3, (self, policy, orElse) => flatMap2(toStepWithMetadata(policy), (step) => {
    let meta = CurrentMetadata.defaultValue();
    let lastError;
    const loop = catchCauseFilter(suspend(() => provideService(self, CurrentMetadata, meta)), findErrorOnly, (error) => {
      lastError = error;
      return flatMap2(step(error), (meta_) => {
        meta = meta_;
        return loop;
      });
    });
    return catchDone(loop, (out) => internalCall(() => orElse(lastError, out)));
  }));
  var repeat = /* @__PURE__ */ dual(2, (self, options) => {
    const schedule4 = typeof options === "function" ? options(identity) : isSchedule(options) ? options : buildFromOptions(options);
    return repeatOrElse(self, schedule4, fail3);
  });
  var retry = /* @__PURE__ */ dual(2, (self, options) => {
    const schedule4 = typeof options === "function" ? options(identity) : isSchedule(options) ? options : buildFromOptions(options);
    return retryOrElse(self, schedule4, fail3);
  });
  var passthroughForever = /* @__PURE__ */ passthrough(forever2);
  var buildFromOptions = /* @__PURE__ */ __name((options) => {
    let schedule4 = options.schedule ? passthrough(options.schedule) : passthroughForever;
    if (options.while) {
      schedule4 = while_(schedule4, ({
        input
      }) => {
        const applied = options.while(input);
        return isEffect(applied) ? applied : succeed3(applied);
      });
    }
    if (options.until) {
      schedule4 = while_(schedule4, ({
        input
      }) => {
        const applied = options.until(input);
        return isEffect(applied) ? map3(applied, (b) => !b) : succeed3(!applied);
      });
    }
    if (options.times !== void 0) {
      schedule4 = while_(schedule4, ({
        attempt
      }) => succeed3(attempt <= options.times));
    }
    return schedule4;
  }, "buildFromOptions");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Effect.js
  var isEffect2 = isEffect;
  var all2 = all;
  var forEach2 = forEach;
  var whileLoop2 = whileLoop;
  var tryPromise2 = tryPromise;
  var succeed7 = succeed3;
  var suspend2 = suspend;
  var sync2 = sync;
  var void_3 = void_;
  var callback2 = callback;
  var never2 = never;
  var gen2 = gen;
  var fail5 = fail3;
  var failCause4 = failCause;
  var failCauseSync2 = failCauseSync;
  var die3 = die;
  var try_2 = try_;
  var withFiber2 = withFiber;
  var fromResult2 = fromResult;
  var fromOption3 = fromOption2;
  var flatMap3 = flatMap2;
  var flatten4 = flatten3;
  var andThen2 = andThen;
  var tap2 = tap;
  var exit2 = exit;
  var map5 = map3;
  var as2 = as;
  var asVoid2 = asVoid;
  var zipWith2 = zipWith;
  var catch_2 = catch_;
  var catchTag2 = catchTag;
  var catchCause2 = catchCause;
  var mapError2 = mapError;
  var retry2 = retry;
  var timeoutOption2 = timeoutOption;
  var sleep2 = sleep;
  var raceFirst2 = raceFirst;
  var filter2 = filter;
  var when2 = when;
  var matchCauseEffect2 = matchCauseEffect;
  var context2 = context;
  var contextWith2 = contextWith;
  var provideContext2 = provideContext;
  var provideService2 = provideService;
  var scope2 = scope;
  var scoped2 = scoped;
  var acquireRelease2 = acquireRelease;
  var acquireUseRelease2 = acquireUseRelease;
  var addFinalizer3 = addFinalizer;
  var ensuring2 = ensuring;
  var onError2 = onError;
  var onExit2 = onExit;
  var interrupt3 = interrupt;
  var onInterrupt2 = onInterrupt;
  var uninterruptible2 = uninterruptible;
  var forever3 = forever;
  var repeat2 = repeat;
  var forkIn2 = forkIn;
  var forkScoped2 = forkScoped;
  var forkDetach2 = forkDetach;
  var fiberId2 = fiberId;
  var runFork2 = runFork;
  var runForkWith2 = runForkWith;
  var runCallbackWith2 = runCallbackWith;
  var runCallback2 = runCallback;
  var runPromise2 = runPromise;
  var runPromiseWith2 = runPromiseWith;
  var runPromiseExit2 = runPromiseExit;
  var runPromiseExitWith2 = runPromiseExitWith;
  var runSync2 = runSync;
  var runSyncWith2 = runSyncWith;
  var runSyncExit2 = runSyncExit;
  var runSyncExitWith2 = runSyncExitWith;
  var fnUntraced2 = fnUntraced;
  var fn2 = fn;
  var logWarning = /* @__PURE__ */ logWithLevel("Warn");
  var logError = /* @__PURE__ */ logWithLevel("Error");
  var logDebug = /* @__PURE__ */ logWithLevel("Debug");
  var annotateLogs = /* @__PURE__ */ dual((args2) => isEffect2(args2[0]), (effect2, ...args2) => updateService(effect2, CurrentLogAnnotations2, (annotations) => {
    const newAnnotations = args2.length === 1 ? {
      ...annotations,
      ...args2[0]
    } : {
      ...annotations
    };
    if (args2.length === 1) {
      return newAnnotations;
    } else {
      assignProperty(newAnnotations, args2[0], args2[1]);
    }
    return newAnnotations;
  }));
  var mapEager2 = mapEager;
  var mapErrorEager2 = mapErrorEager;
  var flatMapEager2 = flatMapEager;
  var fnUntracedEager2 = fnUntracedEager;

  // packages/runtime/src/host/operations.ts
  var operations = /* @__PURE__ */ new Map();
  var operate = /* @__PURE__ */ __name((operation, host4, queueDeadline = Date.now() + 5e3) => callback2((resume) => {
    let id;
    let json2;
    try {
      json2 = JSON.stringify(operation);
      id = host4.prepareOperation();
    } catch (cause) {
      resume(fail5(cause instanceof Error ? cause : new Error(String(cause))));
      return;
    }
    const cancel = /* @__PURE__ */ __name(() => {
      const pending2 = operations.get(id);
      if (pending2 === void 0) return;
      operations.delete(id);
      clearTimeout(pending2.timeout);
      host4.cancelOperation(id);
    }, "cancel");
    const complete = /* @__PURE__ */ __name((answer, error) => {
      const pending2 = operations.get(id);
      if (pending2 === void 0) return;
      operations.delete(id);
      clearTimeout(pending2.timeout);
      resume(answer === null ? fail5(new Error(error ?? "The host operation failed")) : succeed7(answer));
    }, "complete");
    const fail7 = /* @__PURE__ */ __name((cause) => {
      cancel();
      resume(fail5(cause));
    }, "fail");
    const requiresIssuance = operation.kind === "equipConsumable" || operation.kind === "prepareConsumableClaim";
    operations.set(id, {
      complete,
      fail: fail7,
      requiresIssuance,
      timeout: requiresIssuance ? setTimeout(() => fail7(new Error("The equip permit timed out")), Math.max(0, queueDeadline - Date.now())) : void 0
    });
    try {
      host4.operate(id, json2);
    } catch (cause) {
      fail7(cause instanceof Error ? cause : new Error(String(cause)));
    }
    return sync2(cancel);
  }), "operate");
  var setOperationResponsibility = /* @__PURE__ */ __name((id, responsible) => {
    const pending2 = operations.get(id);
    if (pending2 === void 0 || pending2.started) return;
    clearTimeout(pending2.timeout);
    pending2.timeout = responsible ? setTimeout(() => pending2.fail(new Error("The equip permit timed out")), 5e3) : void 0;
  }, "setOperationResponsibility");
  var startOperation = /* @__PURE__ */ __name((id, issued = false) => {
    const pending2 = operations.get(id);
    if (pending2 === void 0 || pending2.started || pending2.requiresIssuance && !issued) return;
    pending2.started = true;
    clearTimeout(pending2.timeout);
    pending2.timeout = setTimeout(() => {
      pending2.fail(new Error("The host operation timed out"));
    }, 35e3);
  }, "startOperation");
  var completeOperation = /* @__PURE__ */ __name((id, answer, error) => operations.get(id)?.complete(answer, error), "completeOperation");
  var failOperations = /* @__PURE__ */ __name((cause) => {
    for (const pending2 of operations.values()) {
      pending2.fail(cause);
    }
  }, "failOperations");

  // packages/runtime/src/host/io.ts
  var pending = /* @__PURE__ */ new Map();
  var requestIo = /* @__PURE__ */ __name((host4, request, signal) => new Promise((resolve3, reject) => {
    if (signal?.aborted) {
      reject(signal.reason);
      return;
    }
    const submitted = JSON.parse(host4.requestIo(JSON.stringify(request)));
    if ("result" in submitted) {
      resolve3(submitted.result);
      return;
    }
    const { id } = submitted;
    const cancel = /* @__PURE__ */ __name(() => {
      pending.delete(id);
      signal?.removeEventListener("abort", cancel);
      host4.cancelIo(id);
      reject(signal?.reason);
    }, "cancel");
    pending.set(id, (result2) => {
      pending.delete(id);
      signal?.removeEventListener("abort", cancel);
      resolve3(result2);
    });
    signal?.addEventListener("abort", cancel, { once: true });
  }), "requestIo");
  var completeIo = /* @__PURE__ */ __name((id, result2) => {
    pending.get(id)?.(result2);
  }, "completeIo");

  // packages/runtime/src/host/jobs.ts
  var profileJobHost = globalThis.__host;
  var jobs = /* @__PURE__ */ new Map();
  var startProfileJob = /* @__PURE__ */ __name((request, host4) => gen2(function* () {
    const id = yield* acquireRelease2(
      sync2(() => {
        const id2 = host4.startJob(JSON.stringify(request));
        jobs.set(id2, { listeners: /* @__PURE__ */ new Set() });
        return id2;
      }),
      (id2) => sync2(() => {
        host4.cancelJob(id2);
        jobs.delete(id2);
      })
    );
    const failed = callback2((resume) => {
      const pending2 = jobs.get(id);
      if (pending2?.error !== void 0) {
        resume(die3(pending2.error));
        return;
      }
      const fail7 = /* @__PURE__ */ __name((error) => resume(die3(error)), "fail");
      pending2?.listeners.add(fail7);
      return sync2(() => {
        pending2?.listeners.delete(fail7);
      });
    });
    return yield* callback2((resume) => {
      const pending2 = jobs.get(id);
      if (pending2 === void 0) {
        resume(die3(new Error("Combat profile job was cancelled")));
        return;
      }
      pending2.prepared = (answer) => {
        pending2.preparation = void 0;
        resume(succeed7({ id, ...answer, failed }));
      };
      const fail7 = /* @__PURE__ */ __name((error) => resume(die3(error)), "fail");
      pending2.listeners.add(fail7);
      if (pending2.error !== void 0) fail7(pending2.error);
      else if (pending2.preparation !== void 0) {
        pending2.prepared(pending2.preparation);
      }
      return sync2(() => {
        pending2.prepared = void 0;
        pending2.listeners.delete(fail7);
      });
    });
  }), "startProfileJob");
  var prepareJob = /* @__PURE__ */ __name((id, label, warnings, classNames) => {
    const pending2 = jobs.get(id);
    if (pending2 === void 0 || pending2.error !== void 0) return;
    const answer = { label, warnings, classNames };
    pending2.preparation = answer;
    pending2.prepared?.(answer);
  }, "prepareJob");
  var failJob = /* @__PURE__ */ __name((id, error) => {
    const pending2 = jobs.get(id);
    if (pending2 === void 0) return;
    pending2.preparation = void 0;
    pending2.error = new Error(error);
    for (const fail7 of pending2.listeners) fail7(pending2.error);
  }, "failJob");
  var failJobs = /* @__PURE__ */ __name((error) => {
    for (const id of jobs.keys()) failJob(id, error.message);
  }, "failJobs");

  // packages/runtime/src/host/loop.ts
  var host3 = __host;
  var timers = /* @__PURE__ */ new Map();
  var nextId = 1;
  var schedule = /* @__PURE__ */ __name((callback3, delay3, args2, repeat4) => {
    const ms = Math.max(0, Number(delay3) || 0);
    const timer = {
      id: nextId++,
      at: host3.now() + ms,
      callback: /* @__PURE__ */ __name(() => callback3(...args2), "callback"),
      ...repeat4 ? { repeat: ms } : {}
    };
    timers.set(timer.id, timer);
    return timer.id;
  }, "schedule");
  var clear = /* @__PURE__ */ __name((id) => {
    if (id !== void 0) timers.delete(id);
  }, "clear");
  var describe = /* @__PURE__ */ __name((cause) => {
    try {
      return cause instanceof Error ? cause.stack ?? String(cause) : String(cause);
    } catch {
      return "a value that cannot be converted to text was thrown";
    }
  }, "describe");
  var turn = /* @__PURE__ */ __name((itemsJson) => {
    const now = host3.now();
    const due = [...timers.values()].filter((timer) => timer.at <= now).sort((a, b) => a.at - b.at || a.id - b.id);
    const errors = [];
    const invoke = /* @__PURE__ */ __name((callback3) => {
      try {
        callback3();
      } catch (cause) {
        errors.push(describe(cause));
      }
    }, "invoke");
    const items = JSON.parse(itemsJson);
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
        host3.log("warn", item.warning);
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
          const callback3 = Reflect.get(window, item.name);
          if (typeof callback3 === "function") callback3.apply(window, item.args);
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
      if (timer.repeat === void 0) {
        timers.delete(timer.id);
      } else {
        timers.set(timer.id, { ...timer, at: host3.now() + timer.repeat });
      }
      invoke(timer.callback);
    }
    return errors;
  }, "turn");
  var wake = /* @__PURE__ */ __name(() => {
    let next = Infinity;
    for (const timer of timers.values()) next = Math.min(next, timer.at);
    return next;
  }, "wake");
  if (typeof window === "undefined") {
    Object.assign(globalThis, { window: globalThis });
  }
  Object.assign(globalThis, {
    setTimeout: /* @__PURE__ */ __name((callback3, delay3, ...args2) => schedule(callback3, delay3, args2, false), "setTimeout"),
    setInterval: /* @__PURE__ */ __name((callback3, delay3, ...args2) => schedule(callback3, delay3, args2, true), "setInterval"),
    clearTimeout: clear,
    clearInterval: clear
  });
  window.swf = new Proxy({}, {
    get(_target, method) {
      if (typeof method === "string") {
        return (...args2) => {
          const reply = host3.swf(method, JSON.stringify(args2));
          return reply === void 0 ? void 0 : JSON.parse(reply);
        };
      }
      return void 0;
    }
  });
  var hideInternals = /* @__PURE__ */ __name(() => {
    Reflect.deleteProperty(globalThis, "__host");
    Reflect.deleteProperty(globalThis, "__lucent");
    Reflect.deleteProperty(window, "swf");
  }, "hideInternals");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Fiber.js
  var await_ = fiberAwait;
  var join = fiberJoin;
  var interrupt4 = fiberInterrupt;
  var runIn = fiberRunIn;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/ManagedRuntime.js
  var TypeId8 = "~effect/ManagedRuntime";
  var make7 = /* @__PURE__ */ __name((layer8, options) => {
    const memoMap = options?.memoMap ?? makeMemoMapUnsafe();
    const scope3 = makeUnsafe3("sequential");
    const layerScope = forkUnsafe2(scope3, "sequential");
    const fiberScope = forkUnsafe2(scope3, "parallel");
    const defaultRunOptions = {
      onFiberStart: runIn(fiberScope)
    };
    const mergeRunOptions = /* @__PURE__ */ __name((options2) => options2 ? {
      ...options2,
      onFiberStart: options2.onFiberStart ? (fiber2) => {
        defaultRunOptions.onFiberStart(fiber2);
        options2.onFiberStart(fiber2);
      } : defaultRunOptions.onFiberStart
    } : defaultRunOptions, "mergeRunOptions");
    let buildFiber;
    const contextEffect = withFiber2((fiber2) => {
      if (!buildFiber) {
        buildFiber = runFork2(tap2(buildWithMemoMap(layer8, memoMap, layerScope), (context3) => sync2(() => {
          self.cachedContext = context3;
        })), {
          ...defaultRunOptions,
          scheduler: fiber2.cache.scheduler
        });
      }
      return flatten4(await_(buildFiber));
    });
    const self = {
      [TypeId8]: TypeId8,
      memoMap,
      scope: scope3,
      contextEffect,
      cachedContext: void 0,
      context() {
        return self.cachedContext === void 0 ? runPromise2(self.contextEffect) : Promise.resolve(self.cachedContext);
      },
      dispose() {
        return runPromise2(self.disposeEffect);
      },
      [Symbol.asyncDispose]() {
        return self.dispose();
      },
      disposeEffect: withFiber2((fiber2) => {
        for (let current = fiber2; current; current = current._parent) {
          scopeRemoveFinalizerUnsafe(fiberScope, current);
        }
        ;
        self.contextEffect = die3("ManagedRuntime disposed");
        self.cachedContext = void 0;
        return close(self.scope, void_2);
      }),
      runFork(effect2, options2) {
        return self.cachedContext === void 0 ? runFork2(provide3(self, effect2), mergeRunOptions(options2)) : runForkWith2(self.cachedContext)(effect2, mergeRunOptions(options2));
      },
      runCallback(effect2, options2) {
        return self.cachedContext === void 0 ? runCallback2(provide3(self, effect2), mergeRunOptions(options2)) : runCallbackWith2(self.cachedContext)(effect2, mergeRunOptions(options2));
      },
      runSyncExit(effect2) {
        return self.cachedContext === void 0 ? runSyncExit2(provide3(self, effect2)) : runSyncExitWith2(self.cachedContext)(effect2);
      },
      runSync(effect2) {
        return self.cachedContext === void 0 ? runSync2(provide3(self, effect2)) : runSyncWith2(self.cachedContext)(effect2);
      },
      runPromiseExit(effect2, options2) {
        return self.cachedContext === void 0 ? runPromiseExit2(provide3(self, effect2), mergeRunOptions(options2)) : runPromiseExitWith2(self.cachedContext)(effect2, mergeRunOptions(options2));
      },
      runPromise(effect2, options2) {
        return self.cachedContext === void 0 ? runPromise2(provide3(self, effect2), mergeRunOptions(options2)) : runPromiseWith2(self.cachedContext)(effect2, mergeRunOptions(options2));
      }
    };
    return self;
  }, "make");
  function provide3(managed, effect2) {
    return flatMap3(managed.contextEffect, (context3) => provideContext2(effect2, context3));
  }
  __name(provide3, "provide");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/encoding/Base64.js
  var encode = /* @__PURE__ */ __name((input) => encodeBytes(typeof input === "string" ? encoder.encode(input) : input), "encode");
  var decode = /* @__PURE__ */ __name((str) => {
    const stripped = stripCrlf(str);
    const length = stripped.length;
    if (length % 4 !== 0) {
      return fail2(new EncodingError({
        kind: "Decode",
        module: "Base64",
        input: stripped,
        message: `Length must be a multiple of 4, but is ${length}`
      }));
    }
    const index = stripped.indexOf("=");
    if (index !== -1 && (index < length - 2 || index === length - 2 && stripped[length - 1] !== "=")) {
      return fail2(new EncodingError({
        kind: "Decode",
        module: "Base64",
        input: stripped,
        message: "Found a '=' character, but it is not at the end"
      }));
    }
    try {
      const missingOctets = stripped.endsWith("==") ? 2 : stripped.endsWith("=") ? 1 : 0;
      const result2 = new Uint8Array(3 * (length / 4) - missingOctets);
      for (let i = 0, j = 0; i < length; i += 4, j += 3) {
        const buffer2 = getCode(stripped.charCodeAt(i)) << 18 | getCode(stripped.charCodeAt(i + 1)) << 12 | getCode(stripped.charCodeAt(i + 2)) << 6 | getCode(stripped.charCodeAt(i + 3));
        result2[j] = buffer2 >> 16;
        result2[j + 1] = buffer2 >> 8 & 255;
        result2[j + 2] = buffer2 & 255;
      }
      return succeed2(result2);
    } catch (cause) {
      return fail2(new EncodingError({
        kind: "Decode",
        module: "Base64",
        input: stripped,
        message: cause instanceof Error ? cause.message : "Invalid input"
      }));
    }
  }, "decode");
  var encodeBytes = /* @__PURE__ */ __name((bytes) => {
    const length = bytes.length;
    let result2 = "";
    let i;
    for (i = 2; i < length; i += 3) {
      result2 += alphabet[bytes[i - 2] >> 2];
      result2 += alphabet[(bytes[i - 2] & 3) << 4 | bytes[i - 1] >> 4];
      result2 += alphabet[(bytes[i - 1] & 15) << 2 | bytes[i] >> 6];
      result2 += alphabet[bytes[i] & 63];
    }
    if (i === length + 1) {
      result2 += alphabet[bytes[i - 2] >> 2];
      result2 += alphabet[(bytes[i - 2] & 3) << 4];
      result2 += "==";
    }
    if (i === length) {
      result2 += alphabet[bytes[i - 2] >> 2];
      result2 += alphabet[(bytes[i - 2] & 3) << 4 | bytes[i - 1] >> 4];
      result2 += alphabet[(bytes[i - 1] & 15) << 2];
      result2 += "=";
    }
    return result2;
  }, "encodeBytes");
  var encoder = /* @__PURE__ */ new TextEncoder();
  var stripCrlf = /* @__PURE__ */ __name((str) => str.replace(/[\n\r]/g, ""), "stripCrlf");
  var alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  var codes = /* @__PURE__ */ new Uint8Array(123).fill(255);
  for (let i = 0; i < alphabet.length; i++) codes[/* @__PURE__ */ alphabet.charCodeAt(i)] = i;
  codes[/* @__PURE__ */ "=".charCodeAt(0)] = 0;
  var getCode = /* @__PURE__ */ __name((charCode) => {
    if (charCode >= codes.length || codes[charCode] === 255) {
      throw new TypeError(`Invalid character ${String.fromCharCode(charCode)}`);
    }
    return codes[charCode];
  }, "getCode");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schema/annotations.js
  function resolve(ast) {
    return ast.checks ? ast.checks[ast.checks.length - 1].annotations : ast.annotations;
  }
  __name(resolve, "resolve");
  var STRUCTURAL_ANNOTATION_KEY = "~structural";
  var SENTINELS_ANNOTATION_KEY = "~sentinels";
  var CONSTRUCTOR_ANNOTATION_KEY = "~constructor";
  var getExpected = /* @__PURE__ */ memoize((ast) => {
    const identifier2 = resolve(ast)?.identifier;
    if (typeof identifier2 === "string") return identifier2;
    return ast.getExpected(getExpected);
  });

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schema/parser.js
  var missing = /* @__PURE__ */ Symbol();
  var succeed8 = succeed4;
  var missingExit = /* @__PURE__ */ succeed8(missing);
  var sameExit = /* @__PURE__ */ succeed8(missing);
  var toOption = /* @__PURE__ */ __name((value) => value === missing ? none2() : some2(value), "toOption");
  var fromOptionExit = /* @__PURE__ */ __name((option2) => option2._tag === "None" ? missingExit : succeed8(option2.value), "fromOptionExit");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/SchemaIssue.js
  var TypeId9 = "~effect/SchemaIssue/Issue";
  function isIssue(u) {
    return hasProperty(u, TypeId9) && u[TypeId9] === TypeId9;
  }
  __name(isIssue, "isIssue");
  function hasInput(issue2) {
    return Object.hasOwn(issue2, "input");
  }
  __name(hasInput, "hasInput");
  var IssueNodeImpl = class {
    static {
      __name(this, "IssueNodeImpl");
    }
    [TypeId9] = TypeId9;
    constructor(input, options) {
      if (options?.reportInput === true && input !== missing) {
        this.input = input;
      }
    }
  };
  var Filter = class extends IssueNodeImpl {
    static {
      __name(this, "Filter");
    }
    _tag = "Filter";
    /**
     * The filter that failed.
     */
    filter;
    /**
     * The issue that occurred.
     */
    issue;
    constructor(filter7, issue2, input, options) {
      super(input, options);
      this.filter = filter7;
      this.issue = issue2;
    }
  };
  var Encoding = class extends IssueNodeImpl {
    static {
      __name(this, "Encoding");
    }
    _tag = "Encoding";
    /**
     * The schema that caused the issue.
     */
    ast;
    /**
     * The issue that occurred.
     */
    issue;
    constructor(ast, issue2, input, options) {
      super(input, options);
      this.ast = ast;
      this.issue = issue2;
    }
  };
  var Pointer = class extends IssueNodeImpl {
    static {
      __name(this, "Pointer");
    }
    _tag = "Pointer";
    /**
     * The path to the location in the input that caused the issue.
     */
    path;
    /**
     * The issue that occurred.
     */
    issue;
    constructor(path, issue2) {
      super();
      this.path = path;
      this.issue = issue2;
    }
  };
  var MissingKey = class extends IssueNodeImpl {
    static {
      __name(this, "MissingKey");
    }
    _tag = "MissingKey";
    /**
     * The metadata for the issue.
     */
    annotations;
    constructor(annotations) {
      super();
      this.annotations = annotations;
    }
  };
  var UnexpectedKey = class extends IssueNodeImpl {
    static {
      __name(this, "UnexpectedKey");
    }
    _tag = "UnexpectedKey";
    /**
     * The schema that caused the issue.
     */
    ast;
    constructor(ast, input, options) {
      super(input, options);
      this.ast = ast;
    }
  };
  var Composite = class extends IssueNodeImpl {
    static {
      __name(this, "Composite");
    }
    _tag = "Composite";
    /**
     * The schema that caused the issue.
     */
    ast;
    /**
     * The issues that occurred.
     */
    issues;
    constructor(ast, issues, input, options) {
      super(input, options);
      this.ast = ast;
      this.issues = issues;
    }
  };
  var InvalidType = class extends IssueNodeImpl {
    static {
      __name(this, "InvalidType");
    }
    _tag = "InvalidType";
    /**
     * The schema that caused the issue.
     */
    ast;
    constructor(ast, input, options) {
      super(input, options);
      this.ast = ast;
    }
  };
  var InvalidValue = class extends IssueNodeImpl {
    static {
      __name(this, "InvalidValue");
    }
    _tag = "InvalidValue";
    /**
     * The metadata for the issue.
     */
    annotations;
    constructor(annotations, input, options) {
      super(input, options);
      this.annotations = annotations;
    }
  };
  var AnyOf = class extends IssueNodeImpl {
    static {
      __name(this, "AnyOf");
    }
    _tag = "AnyOf";
    /**
     * The schema that caused the issue.
     */
    ast;
    /**
     * The issues that occurred.
     */
    issues;
    constructor(ast, issues, input, options) {
      super(input, options);
      this.ast = ast;
      this.issues = issues;
    }
  };
  var OneOf = class extends IssueNodeImpl {
    static {
      __name(this, "OneOf");
    }
    _tag = "OneOf";
    /**
     * The schema that caused the issue.
     */
    ast;
    /**
     * The schemas that were successful.
     */
    successes;
    constructor(ast, successes, input, options) {
      super(input, options);
      this.ast = ast;
      this.successes = successes;
    }
  };
  function makeFilterIssue(entry, input, options) {
    if (isIssue(entry)) {
      return entry;
    }
    if (typeof entry === "string") {
      return new InvalidValue({
        message: entry
      }, input, options);
    }
    const inner = typeof entry.issue === "string" ? new InvalidValue({
      message: entry.issue
    }, input, options) : entry.issue;
    return new Pointer(entry.path, inner);
  }
  __name(makeFilterIssue, "makeFilterIssue");
  function makeSingle(out, input, options) {
    if (out === void 0) {
      return void 0;
    }
    if (typeof out === "boolean") {
      return out ? void 0 : new InvalidValue(void 0, input, options);
    }
    return makeFilterIssue(out, input, options);
  }
  __name(makeSingle, "makeSingle");
  function normalizeFilterOutput(ast, out, input, options) {
    if (Array.isArray(out)) {
      if (!isReadonlyArrayNonEmpty(out)) {
        return void 0;
      }
      return out.length === 1 ? makeFilterIssue(out[0], input, options) : new Composite(ast, map2(out, (entry) => makeFilterIssue(entry, input, options)), input, options);
    }
    return makeSingle(out, input, options);
  }
  __name(normalizeFilterOutput, "normalizeFilterOutput");
  var defaultLeafHook = /* @__PURE__ */ __name((issue2) => {
    const message2 = findMessage(issue2);
    if (message2 !== void 0) return message2;
    switch (issue2._tag) {
      case "InvalidType":
        return getExpectedMessage(getExpected(issue2.ast), issue2);
      case "InvalidValue": {
        const expected = findExpected(issue2);
        if (expected !== void 0) return getExpectedMessage(expected, issue2);
        const input = formatInput(issue2);
        return input === void 0 ? "Expected a valid value" : `Invalid data ${input}`;
      }
      case "MissingKey":
        return "Missing key";
      case "UnexpectedKey": {
        const input = formatInput(issue2);
        return input === void 0 ? "Expected no excess property" : `Unexpected key with value ${input}`;
      }
      case "Forbidden":
        return "Forbidden operation";
      case "OneOf": {
        const input = formatInput(issue2);
        return input === void 0 ? "Expected exactly one member to match" : `Expected exactly one member to match the input ${input}`;
      }
    }
  }, "defaultLeafHook");
  var defaultCheckHook = /* @__PURE__ */ __name((issue2) => findMessage(issue2.issue) ?? findMessage(issue2), "defaultCheckHook");
  function formatInput(issue2) {
    return hasInput(issue2) ? format2(issue2.input) : void 0;
  }
  __name(formatInput, "formatInput");
  function findExpected(issue2) {
    const expected = issue2.annotations?.expected;
    return typeof expected === "string" ? expected : void 0;
  }
  __name(findExpected, "findExpected");
  function getExpectedMessage(expected, issue2) {
    const input = formatInput(issue2);
    return input === void 0 ? `Expected ${expected}` : `Expected ${expected}, got ${input}`;
  }
  __name(getExpectedMessage, "getExpectedMessage");
  function formatCheck(check) {
    const expected = check.annotations?.expected;
    if (typeof expected === "string") return expected;
    switch (check._tag) {
      case "Filter":
        return "<filter>";
      case "FilterGroup":
        return check.checks.map((check2) => formatCheck(check2)).join(" & ");
    }
  }
  __name(formatCheck, "formatCheck");
  function makeFormatterDefault() {
    return (issue2) => formatIssue(issue2, "");
  }
  __name(makeFormatterDefault, "makeFormatterDefault");
  var defaultFormatter = /* @__PURE__ */ makeFormatterDefault();
  function formatIssue(issue2, path) {
    let message2;
    switch (issue2._tag) {
      case "Filter": {
        const annotated = defaultCheckHook(issue2);
        if (annotated !== void 0) {
          message2 = annotated;
        } else {
          if (issue2.issue._tag !== "InvalidValue") {
            return formatIssue(issue2.issue, path);
          }
          const expected = findExpected(issue2.issue);
          message2 = expected === void 0 ? getExpectedMessage(formatCheck(issue2.filter), issue2) : getExpectedMessage(expected, issue2.issue);
        }
        break;
      }
      case "Encoding":
        return formatIssue(issue2.issue, path);
      case "Pointer":
        return formatIssue(issue2.issue, path + formatPath(issue2.path));
      case "Composite":
      case "AnyOf": {
        if (issue2._tag === "Composite" || issue2.issues.length > 0) {
          return issue2.issues.map((issue3) => formatIssue(issue3, path)).join("\n");
        }
        message2 = findMessage(issue2) ?? getExpectedMessage(getExpected(issue2.ast), issue2);
        break;
      }
      default:
        message2 = defaultLeafHook(issue2);
        break;
    }
    return path ? `${message2}
  at ${path}` : message2;
  }
  __name(formatIssue, "formatIssue");
  function findMessage(issue2) {
    if (issue2._tag === "Pointer") return;
    if (issue2._tag === "Encoding") return findMessage(issue2.issue);
    const annotations = issue2._tag === "Filter" ? issue2.filter.annotations : "annotations" in issue2 ? issue2.annotations : issue2.ast.annotations;
    const message2 = annotations?.[issue2._tag === "MissingKey" ? "messageMissingKey" : issue2._tag === "UnexpectedKey" ? "messageUnexpectedKey" : "message"];
    if (typeof message2 === "string") return message2;
  }
  __name(findMessage, "findMessage");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schema/cause.js
  function getSchemaIssue(cause) {
    let issue2;
    for (const reason of cause.reasons) {
      if (!isFailReason2(reason) || !isIssue(reason.error)) {
        return void 0;
      }
      issue2 ??= reason.error;
    }
    return issue2;
  }
  __name(getSchemaIssue, "getSchemaIssue");
  function getSchemaIssueOrThrow(cause, message2) {
    const issue2 = getSchemaIssue(cause);
    if (issue2 === void 0) {
      throw new Error(message2, {
        cause
      });
    }
    return issue2;
  }
  __name(getSchemaIssueOrThrow, "getSchemaIssueOrThrow");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/SchemaGetter.js
  var makeGetter = /* @__PURE__ */ __name((fields) => Object.assign(Object.create(Prototype), fields), "makeGetter");
  var passthrough_ = /* @__PURE__ */ makeGetter({
    _tag: "Passthrough"
  });
  function passthrough2() {
    return passthrough_;
  }
  __name(passthrough2, "passthrough");
  function transform(f) {
    return makeGetter({
      _tag: "Transform",
      transform: f
    });
  }
  __name(transform, "transform");
  function transformEffect(f) {
    return makeGetter({
      _tag: "TransformEffect",
      transform: f
    });
  }
  __name(transformEffect, "transformEffect");
  function String2() {
    return transform(globalThis.String);
  }
  __name(String2, "String");
  function Number4() {
    return transform(globalThis.Number);
  }
  __name(Number4, "Number");
  function Date2() {
    return transform((u) => new globalThis.Date(u));
  }
  __name(Date2, "Date");
  function encodeBase64() {
    return transform(encode);
  }
  __name(encodeBase64, "encodeBase64");
  function decodeBase64() {
    return transformEffect((input, options) => mapErrorEager2(fromResult2(decode(input)), () => new InvalidValue({
      expected: "a valid Base64 string"
    }, input, options)));
  }
  __name(decodeBase64, "decodeBase64");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/SchemaTransformation.js
  var TypeId10 = "~effect/SchemaTransformation/Transformation";
  var Transformation = class extends Class {
    static {
      __name(this, "Transformation");
    }
    [TypeId10] = TypeId10;
    _tag = "Transformation";
    decode;
    encode;
    constructor(decode3, encode2) {
      super();
      this.decode = decode3;
      this.encode = encode2;
    }
    flip() {
      return new Transformation(this.encode, this.decode);
    }
  };
  function isTransformation(u) {
    return hasProperty(u, TypeId10) && u[TypeId10] === TypeId10;
  }
  __name(isTransformation, "isTransformation");
  var makeTransformation = /* @__PURE__ */ __name((options) => {
    if (isTransformation(options)) {
      return options;
    }
    return new Transformation(options.decode, options.encode);
  }, "makeTransformation");
  function transformEffect2(options) {
    return new Transformation(transformEffect(options.decode), transformEffect(options.encode));
  }
  __name(transformEffect2, "transformEffect");
  function transform2(options) {
    return new Transformation(transform(options.decode), transform(options.encode));
  }
  __name(transform2, "transform");
  var passthrough_2 = /* @__PURE__ */ new Transformation(/* @__PURE__ */ passthrough2(), /* @__PURE__ */ passthrough2());
  function passthrough3() {
    return passthrough_2;
  }
  __name(passthrough3, "passthrough");
  var numberFromString = /* @__PURE__ */ new Transformation(/* @__PURE__ */ Number4(), /* @__PURE__ */ String2());
  var dateFromString = /* @__PURE__ */ new Transformation(/* @__PURE__ */ Date2(), /* @__PURE__ */ transform(formatDate));
  var isJsonError = /* @__PURE__ */ __name((input) => isObject(input) && typeof input["message"] === "string", "isJsonError");
  var decodeJsonError = /* @__PURE__ */ __name((input) => {
    const hasCause = Object.hasOwn(input, "cause");
    const err = hasCause ? new Error(input.message, {
      cause: decodeDefect(input.cause)
    }) : new Error(input.message);
    if (typeof input.name === "string" && input.name !== "Error") err.name = input.name;
    if (typeof input.stack === "string") err.stack = input.stack;
    return err;
  }, "decodeJsonError");
  var encodeUnknownAsJson = /* @__PURE__ */ __name((input) => {
    try {
      const json2 = formatJson(input);
      return json2 === void 0 ? format2(input) : JSON.parse(json2);
    } catch {
      return format2(input);
    }
  }, "encodeUnknownAsJson");
  var encodeJsonError = /* @__PURE__ */ __name((input, options, encodeDefect) => {
    const encoded = {
      name: input.name,
      message: typeof input.message === "string" ? input.message : ""
    };
    if (options?.includeStack && typeof input.stack === "string") {
      encoded.stack = input.stack;
    }
    if (!options?.excludeCause && input.cause !== void 0) {
      encoded.cause = encodeDefect(input.cause);
    }
    return encoded;
  }, "encodeJsonError");
  var makeEncodeDefect = /* @__PURE__ */ __name((options) => {
    const seen = /* @__PURE__ */ new WeakSet();
    const encode2 = /* @__PURE__ */ __name((input) => {
      if (isError(input)) {
        if (seen.has(input)) {
          return "[Circular]";
        }
        seen.add(input);
        const encoded = encodeJsonError(input, options, encode2);
        seen.delete(input);
        return encoded;
      }
      return encodeUnknownAsJson(input);
    }, "encode");
    return encode2;
  }, "makeEncodeDefect");
  var decodeDefect = /* @__PURE__ */ __name((input) => isJsonError(input) ? decodeJsonError(input) : input, "decodeDefect");
  var defectFromJson = /* @__PURE__ */ __name((options) => transform2({
    decode: decodeDefect,
    encode: makeEncodeDefect(options)
  }), "defectFromJson");
  var urlFromString = /* @__PURE__ */ transformEffect2({
    decode: /* @__PURE__ */ __name((s, options) => URL.canParse(s) ? succeed7(new URL(s)) : fail5(new InvalidValue({
      expected: "a valid URL string"
    }, s, options)), "decode"),
    encode: /* @__PURE__ */ __name((url) => succeed7(url.href), "encode")
  });
  var uint8ArrayFromBase64String = /* @__PURE__ */ new Transformation(/* @__PURE__ */ decodeBase64(), /* @__PURE__ */ encodeBase64());

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/SchemaAST.js
  function makeGuard(tag2) {
    return (ast) => ast._tag === tag2;
  }
  __name(makeGuard, "makeGuard");
  var isDeclaration = /* @__PURE__ */ makeGuard("Declaration");
  var isNever2 = /* @__PURE__ */ makeGuard("Never");
  var isLiteral = /* @__PURE__ */ makeGuard("Literal");
  var isUniqueSymbol = /* @__PURE__ */ makeGuard("UniqueSymbol");
  var isArrays = /* @__PURE__ */ makeGuard("Arrays");
  var isObjects = /* @__PURE__ */ makeGuard("Objects");
  var isSuspend = /* @__PURE__ */ makeGuard("Suspend");
  var Link = class {
    static {
      __name(this, "Link");
    }
    to;
    transformation;
    constructor(to, transformation) {
      this.to = to;
      this.transformation = transformation;
    }
  };
  var defaultParseOptions = {};
  var Context = class {
    static {
      __name(this, "Context");
    }
    isOptional;
    isMutable;
    /** Used for constructor default values (e.g. `withConstructorDefault` API) */
    constructorDefault;
    annotations;
    constructor(isOptional2, isMutable, constructorDefault = void 0, annotations = void 0) {
      this.isOptional = isOptional2;
      this.isMutable = isMutable;
      this.constructorDefault = constructorDefault;
      this.annotations = annotations;
    }
  };
  var TypeId11 = "~effect/Schema";
  var ASTNodeImpl = class {
    static {
      __name(this, "ASTNodeImpl");
    }
    [TypeId11] = TypeId11;
    annotations;
    checks;
    encoding;
    context;
    constructor(annotations = void 0, checks = void 0, encoding = void 0, context3 = void 0) {
      this.annotations = annotations;
      this.checks = checks;
      this.encoding = encoding;
      this.context = context3;
    }
    toString() {
      return `<${this._tag}>`;
    }
  };
  var Declaration = class extends ASTNodeImpl {
    static {
      __name(this, "Declaration");
    }
    _tag = "Declaration";
    typeParameters;
    run;
    encodingChecks;
    /**
     * Parser factory {@link flip} swaps in, so a declaration can behave
     * differently when encoding. `undefined` reuses `run`.
     */
    encodingRun;
    constructor(typeParameters, run2, annotations, checks, encoding, context3, encodingChecks, encodingRun) {
      super(annotations, checks, encoding, context3);
      this.typeParameters = typeParameters;
      this.run = run2;
      this.encodingChecks = encodingChecks;
      this.encodingRun = encodingRun;
    }
    /** @internal */
    getParser() {
      let run2;
      return (input, options) => {
        if (input === missing) return missingExit;
        return (run2 ??= this.run(this.typeParameters))(input, this, options);
      };
    }
    _rebuild(recur, checks, encodingChecks, run2, encodingRun) {
      const tps = mapOrSame(this.typeParameters, recur);
      return tps === this.typeParameters && checks === this.checks && encodingChecks === this.encodingChecks && run2 === this.run && encodingRun === this.encodingRun ? this : new Declaration(tps, run2, this.annotations, checks, void 0, this.context, encodingChecks, encodingRun);
    }
    /** @internal */
    recur(recur) {
      return this._rebuild(recur, this.checks, this.encodingChecks, this.run, this.encodingRun);
    }
    /** @internal */
    flip(recur) {
      return this._rebuild(recur, this.encodingChecks, this.checks, this.encodingRun ?? this.run, this.run);
    }
    /** @internal */
    getExpected() {
      const expected = this.annotations?.expected;
      if (typeof expected === "string") return expected;
      return "<Declaration>";
    }
  };
  var Null = class extends ASTNodeImpl {
    static {
      __name(this, "Null");
    }
    _tag = "Null";
    /** @internal */
    getParser() {
      return fromConst(this, null);
    }
    /** @internal */
    getExpected() {
      return "null";
    }
  };
  var null_ = /* @__PURE__ */ new Null();
  var Undefined = class extends ASTNodeImpl {
    static {
      __name(this, "Undefined");
    }
    _tag = "Undefined";
    /** @internal */
    getParser() {
      return fromConst(this, void 0);
    }
    /** @internal */
    toCodecJson() {
      return replaceEncoding(this, [undefinedToNull]);
    }
    /** @internal */
    getExpected() {
      return "undefined";
    }
  };
  var undefinedToNull = /* @__PURE__ */ new Link(null_, /* @__PURE__ */ new Transformation(/* @__PURE__ */ transform(() => void 0), /* @__PURE__ */ transform(() => null)));
  var undefined_2 = /* @__PURE__ */ new Undefined();
  var Void = class extends ASTNodeImpl {
    static {
      __name(this, "Void");
    }
    _tag = "Void";
    /** @internal */
    getParser() {
      const succeed10 = succeed8(void 0);
      return (input) => input === missing ? missingExit : succeed10;
    }
    /** @internal */
    toCodecJson() {
      return replaceEncoding(this, [undefinedToNull]);
    }
    /** @internal */
    getExpected() {
      return "void";
    }
  };
  var void_4 = /* @__PURE__ */ new Void();
  var Never = class extends ASTNodeImpl {
    static {
      __name(this, "Never");
    }
    _tag = "Never";
    /** @internal */
    getParser() {
      return fromRefinement(this, isNever);
    }
    /** @internal */
    getExpected() {
      return "never";
    }
  };
  var never3 = /* @__PURE__ */ new Never();
  var Unknown = class extends ASTNodeImpl {
    static {
      __name(this, "Unknown");
    }
    _tag = "Unknown";
    /** @internal */
    getParser() {
      return fromRefinement(this, isUnknown);
    }
    /** @internal */
    getExpected() {
      return "unknown";
    }
  };
  var unknown = /* @__PURE__ */ new Unknown();
  var Literal = class extends ASTNodeImpl {
    static {
      __name(this, "Literal");
    }
    _tag = "Literal";
    literal;
    constructor(literal2, annotations, checks, encoding, context3) {
      super(annotations, checks, encoding, context3);
      if (typeof literal2 === "number" && !globalThis.Number.isFinite(literal2)) {
        throw new Error(`A numeric literal must be finite, got ${format2(literal2)}`);
      }
      this.literal = literal2;
    }
    /** @internal */
    getParser() {
      return fromConst(this, this.literal);
    }
    /** @internal */
    matchPart(s, _options) {
      return s === globalThis.String(this.literal) ? this.literal : void 0;
    }
    /** @internal */
    toCodecJson() {
      return typeof this.literal === "bigint" ? literalToString(this) : this;
    }
    /** @internal */
    toCodecStringTree() {
      return typeof this.literal === "string" ? this : literalToString(this);
    }
    /** @internal */
    getExpected() {
      return typeof this.literal === "string" ? JSON.stringify(this.literal) : globalThis.String(this.literal);
    }
  };
  function literalToString(ast) {
    const literalAsString = globalThis.String(ast.literal);
    return replaceEncoding(ast, [new Link(new Literal(literalAsString), new Transformation(transform(() => ast.literal), transform(() => literalAsString)))]);
  }
  __name(literalToString, "literalToString");
  var String3 = class extends ASTNodeImpl {
    static {
      __name(this, "String");
    }
    _tag = "String";
    /** @internal */
    getParser() {
      return fromRefinement(this, isString);
    }
    /** @internal */
    matchPart(s, options) {
      const checks = this.checks;
      return checks && !options.disableChecks && collectIssues(checks, s, void 0, this, options) ? void 0 : s;
    }
    /** @internal */
    getExpected() {
      return "string";
    }
  };
  var string2 = /* @__PURE__ */ new String3();
  var Number5 = class extends ASTNodeImpl {
    static {
      __name(this, "Number");
    }
    _tag = "Number";
    /** @internal */
    getParser() {
      return fromRefinement(this, isNumber);
    }
    /** @internal */
    matchKey(s, options) {
      return this._match(isStringNumberRegExp, s, options);
    }
    /** @internal */
    matchPart(s, options) {
      return this._match(isStringFiniteRegExp, s, options);
    }
    _match(regexp, s, options) {
      if (!regexp.test(s)) return void 0;
      const value = globalThis.Number(s);
      if (options.disableChecks || !this.checks) return value;
      return collectIssues(this.checks, value, void 0, this, options) ? void 0 : value;
    }
    /** @internal */
    toCodecJson() {
      if (this.checks && (hasCheck(this.checks, "effect/schema/isFinite") || hasCheck(this.checks, "effect/schema/isInt"))) {
        return this;
      }
      return replaceEncoding(this, [numberToJson]);
    }
    /** @internal */
    toCodecStringTree() {
      if (this.toCodecJson() === this) {
        return replaceEncoding(this, [finiteToString]);
      }
      return replaceEncoding(this, [numberToString]);
    }
    /** @internal */
    getExpected() {
      return "number";
    }
  };
  function hasCheck(checks, id) {
    return checks.some((check) => check.annotations?.representation?.id === id || check._tag === "FilterGroup" && hasCheck(check.checks, id));
  }
  __name(hasCheck, "hasCheck");
  var number2 = /* @__PURE__ */ new Number5();
  var Boolean2 = class extends ASTNodeImpl {
    static {
      __name(this, "Boolean");
    }
    _tag = "Boolean";
    /** @internal */
    getParser() {
      return fromRefinement(this, isBoolean);
    }
    /** @internal */
    getExpected() {
      return "boolean";
    }
  };
  var boolean = /* @__PURE__ */ new Boolean2();
  var Arrays = class extends ASTNodeImpl {
    static {
      __name(this, "Arrays");
    }
    _tag = "Arrays";
    isMutable;
    elements;
    rest;
    encodingChecks;
    constructor(isMutable, elements, rest, annotations, checks, encoding, context3, encodingChecks) {
      super(annotations, checks, encoding, context3);
      this.isMutable = isMutable;
      this.elements = elements;
      this.rest = rest;
      this.encodingChecks = encodingChecks;
      let hasOptional = false;
      for (let i = 0; i < elements.length; i++) {
        if (isOptional(elements[i])) {
          hasOptional = true;
        } else if (hasOptional) {
          throw new Error("A required element cannot follow an optional element. ts(1257)");
        }
      }
      if (hasOptional && rest.length > 1) {
        throw new Error("A required element cannot follow an optional element. ts(1257)");
      }
      for (let i = 1; i < rest.length; i++) {
        if (isOptional(rest[i])) {
          throw new Error("An optional element cannot follow a rest element. ts(1266)");
        }
      }
    }
    /** @internal */
    getParser(compile2, compileField2 = compile2) {
      const ast = this;
      let elements;
      let rest;
      const elementLen = ast.elements.length;
      const tailLen = Math.max(0, ast.rest.length - 1);
      function getParser(tailThreshold, index) {
        if (index < elementLen) {
          return elements[index];
        } else if (index >= tailThreshold) {
          return rest[index - tailThreshold + 1];
        }
        return rest[0];
      }
      __name(getParser, "getParser");
      const finish = /* @__PURE__ */ __name((state) => {
        const {
          input,
          len,
          options
        } = state;
        if (ast.rest.length === 0 && len > elementLen) {
          for (let i = elementLen; i < len; i++) {
            const unexpected = new UnexpectedKey(ast, input[i], options);
            const issue2 = new Pointer([i], unexpected);
            if (options.errors === "all") {
              if (state.issues) state.issues.push(issue2);
              else state.issues = [issue2];
            } else {
              return fail5(new Composite(ast, [issue2], input, options));
            }
          }
        }
        if (state.issues) {
          return fail5(new Composite(ast, state.issues, input, options));
        }
        return succeed8(state.output);
      }, "finish");
      const parse = /* @__PURE__ */ __name((input, options) => {
        const len = input.length;
        const state = {
          ast,
          getParser,
          input,
          len,
          tailThreshold: Math.max(elementLen, len - tailLen),
          output: new globalThis.Array(len),
          issues: void 0,
          options
        };
        const end2 = ast.rest.length === 0 ? elementLen : Math.max(len, elementLen + tailLen);
        const concurrency = options.concurrency === void 0 ? 1 : resolveConcurrency(options.concurrency);
        const eff = concurrency === 1 ? parseArray(state, input, 0, end2) : parseArrayConcurrent(state, input, {
          concurrency,
          end: end2
        });
        if (!eff) return finish(state);
        if (effectIsExit(eff)) return flatMapEager2(eff, () => finish(state));
        let first = true;
        return suspend2(() => {
          if (!first) return parse(input, options);
          first = false;
          return flatMap3(eff, () => finish(state));
        });
      }, "parse");
      return (input, options) => {
        if (input === missing) return missingExit;
        try {
          if (!Array.isArray(input)) {
            return fail5(new InvalidType(ast, input, options));
          }
          if (!elements) {
            elements = ast.elements.map((ast2) => ({
              ast: ast2,
              parser: compileField2(ast2)
            }));
            rest = ast.rest.map((ast2) => ({
              ast: ast2,
              parser: compileField2(ast2)
            }));
          }
          return parse(input, options);
        } catch (error) {
          return die3(error);
        }
      };
    }
    _rebuild(recur, checks, encodingChecks) {
      const elements = mapOrSame(this.elements, recur);
      const rest = mapOrSame(this.rest, recur);
      return elements === this.elements && rest === this.rest && checks === this.checks && encodingChecks === this.encodingChecks ? this : new Arrays(this.isMutable, elements, rest, this.annotations, checks, void 0, this.context, encodingChecks);
    }
    /** @internal */
    recur(recur) {
      return this._rebuild(recur, this.checks, this.encodingChecks);
    }
    /** @internal */
    flip(recur) {
      return this._rebuild(recur, this.encodingChecks, this.checks);
    }
    /** @internal */
    getExpected() {
      return "array";
    }
  };
  function stepArray(s, item, exit3, i) {
    if (exit3._tag === "Failure") {
      return wrapPropertyKeyIssue(s, s.ast, i, exit3);
    }
    const value = exit3 === sameExit ? item : exit3[args];
    if (value !== missing) {
      s.output[i] = value;
    } else {
      const p = s.getParser(s.tailThreshold, i);
      if (isOptional(p.ast)) return;
      const issue2 = new Pointer([i], new MissingKey(p.ast.context?.annotations));
      if (s.options.errors === "all") {
        if (s.issues) s.issues.push(issue2);
        else s.issues = [issue2];
      } else {
        return fail4(new Composite(s.ast, [issue2], s.input, s.options));
      }
    }
  }
  __name(stepArray, "stepArray");
  var parseArrayOptions = {
    onItem(s, item, i) {
      const value = i < s.len ? item : missing;
      return s.getParser(s.tailThreshold, i).parser(value, s.options);
    },
    step: stepArray
  };
  var parseArray = /* @__PURE__ */ iterateEager()(parseArrayOptions);
  var parseArrayConcurrent = /* @__PURE__ */ iterateConcurrent()(parseArrayOptions);
  var wrapPropertyKeyIssue = /* @__PURE__ */ __name((s, ast, key, exit3) => {
    if (exit3.cause.reasons.length === 0) {
      return exit3;
    }
    const issue2 = getSchemaIssue(exit3.cause);
    if (issue2 === void 0) {
      return failCause2(map4(exit3.cause, (issue3) => new Composite(ast, [new Pointer([key], issue3)], s.input, s.options)));
    }
    const pointer = new Pointer([key], issue2);
    if (s.options.errors === "all") {
      if (s.issues) s.issues.push(pointer);
      else s.issues = [pointer];
    } else {
      return fail4(new Composite(ast, [pointer], s.input, s.options));
    }
  }, "wrapPropertyKeyIssue");
  var FINITE_PATTERN = "[+-]?\\d*\\.?\\d+(?:[Ee][+-]?\\d+)?";
  function getIndexSignatureKeys(input, parameter, options = defaultParseOptions) {
    let stringKeys;
    let symbolKeys;
    function go(parameter2) {
      switch (parameter2._tag) {
        case "String":
        case "TemplateLiteral":
          return (stringKeys ??= Object.keys(input)).filter((k) => parameter2.matchPart(k, options) !== void 0);
        case "Number":
          return (stringKeys ??= Object.keys(input)).filter((k) => parameter2.matchKey(k, options) !== void 0);
        case "Symbol":
          return (symbolKeys ??= Object.getOwnPropertySymbols(input)).filter((k) => Object.prototype.propertyIsEnumerable.call(input, k) && parameter2.matchKey(k, options) !== void 0);
        case "Union":
          return [...new Set(parameter2.types.flatMap(go))];
        default:
          return [];
      }
    }
    __name(go, "go");
    return go(parameterFromPropertyKey(toEncoded(parameter)));
  }
  __name(getIndexSignatureKeys, "getIndexSignatureKeys");
  var PropertySignature = class {
    static {
      __name(this, "PropertySignature");
    }
    name;
    type;
    constructor(name, type) {
      this.name = name;
      this.type = type;
    }
  };
  function isIndexSignatureParameterSide(ast) {
    switch (ast._tag) {
      case "String":
      case "Number":
      case "Symbol":
      case "TemplateLiteral":
        return true;
      case "Union":
        return ast.types.every(isIndexSignatureParameterSide);
      default:
        return false;
    }
  }
  __name(isIndexSignatureParameterSide, "isIndexSignatureParameterSide");
  function isIndexSignatureParameterEncodedSide(ast) {
    const encoded = getLastEncoding(ast);
    switch (encoded._tag) {
      case "String":
      case "Number":
      case "Symbol":
      case "TemplateLiteral":
        return true;
      case "Union":
        return encoded.types.every(isIndexSignatureParameterEncodedSide);
      default:
        return false;
    }
  }
  __name(isIndexSignatureParameterEncodedSide, "isIndexSignatureParameterEncodedSide");
  function isIndexSignatureParameter(ast) {
    return isIndexSignatureParameterSide(ast) && isIndexSignatureParameterEncodedSide(ast);
  }
  __name(isIndexSignatureParameter, "isIndexSignatureParameter");
  var IndexSignature = class {
    static {
      __name(this, "IndexSignature");
    }
    parameter;
    type;
    constructor(parameter, type) {
      if (!isIndexSignatureParameter(parameter)) {
        throw new Error(`Invalid index signature parameter ${parameter._tag}`);
      }
      this.parameter = parameter;
      this.type = type;
      if (isOptional(type) && !containsUndefined(type)) {
        throw new Error("Cannot use `Schema.optionalKey` with index signatures, use `Schema.optional` instead.");
      }
    }
  };
  var Objects = class extends ASTNodeImpl {
    static {
      __name(this, "Objects");
    }
    _tag = "Objects";
    propertySignatures;
    indexSignatures;
    encodingChecks;
    constructor(propertySignatures, indexSignatures, annotations, checks, encoding, context3, encodingChecks) {
      super(annotations, checks, encoding, context3);
      this.propertySignatures = propertySignatures;
      this.indexSignatures = indexSignatures;
      this.encodingChecks = encodingChecks;
    }
    /** @internal */
    getParser(compile2, compileField2 = compile2) {
      const ast = this;
      const hasProperties = ast.propertySignatures.length;
      const indexCount = ast.indexSignatures.length;
      if (!hasProperties && !indexCount) {
        return fromRefinement(ast, isNotNullish);
      }
      let properties;
      let indexes;
      const compileMembers = /* @__PURE__ */ __name(() => {
        if (!properties) {
          properties = ast.propertySignatures.map((ps) => ({
            parser: compileField2(ps.type),
            name: ps.name,
            type: ps.type
          }));
          indexes = indexCount ? ast.indexSignatures.map((is3) => ({
            is: is3,
            parserKey: compile2(parameterFromPropertyKey(is3.parameter)),
            parserValue: compileField2(is3.type)
          })) : void 0;
        }
        return properties;
      }, "compileMembers");
      const makeFallback = /* @__PURE__ */ __name(() => {
        const expectedKeys = new Set(ast.propertySignatures.map((ps) => typeof ps.name === "number" ? globalThis.String(ps.name) : ps.name));
        const finishIndex = /* @__PURE__ */ __name((s, key, k2, inputValue, exitValue) => {
          if (exitValue._tag === "Failure") {
            return wrapPropertyKeyIssue(s, ast, key, exitValue) ?? void_2;
          }
          const value = exitValue === sameExit ? inputValue : exitValue[args];
          if (k2 !== missing && value !== missing) {
            if (hasProperties && (expectedKeys.has(key) || expectedKeys.has(typeof k2 === "number" ? globalThis.String(k2) : k2))) return void_2;
            assignProperty(s.out, k2, value);
          }
          return void_2;
        }, "finishIndex");
        const parseIndex = /* @__PURE__ */ __name((s, key, index, exitKey) => {
          if (!exitKey) {
            const eff = index.parserKey(key, s.options);
            if (!effectIsExit(eff)) {
              return flatMap3(exit2(eff), (exit3) => parseIndex(s, key, index, exit3));
            }
            exitKey = eff;
          }
          if (exitKey._tag === "Failure") {
            return wrapPropertyKeyIssue(s, ast, key, exitKey) ?? void_2;
          }
          const k2 = exitKey === sameExit ? key : exitKey[args];
          const inputValue = s.input[key];
          const result2 = index.parserValue(inputValue, s.options);
          return effectIsExit(result2) ? finishIndex(s, key, k2, inputValue, result2) : flatMap3(exit2(result2), (exit3) => finishIndex(s, key, k2, inputValue, exit3));
        }, "parseIndex");
        const parseStringIndex = /* @__PURE__ */ __name((s, key, index) => {
          const inputValue = s.input[key];
          const result2 = index.parserValue(inputValue, s.options);
          return effectIsExit(result2) ? finishIndex(s, key, key, inputValue, result2) : flatMap3(exit2(result2), (exit3) => finishIndex(s, key, key, inputValue, exit3));
        }, "parseStringIndex");
        const parseIndexes = indexCount ? iterateConcurrent()({
          onItem: /* @__PURE__ */ __name((s, [key, index]) => index.is.parameter === string2 ? parseStringIndex(s, key, index) : parseIndex(s, key, index), "onItem"),
          step: /* @__PURE__ */ __name((_s, _item, exit3) => exit3._tag === "Failure" ? exit3 : void 0, "step")
        }) : void 0;
        return fnUntracedEager2(function* (input, options) {
          if (input === missing) {
            return missing;
          }
          if (!(typeof input === "object" && input !== null && !Array.isArray(input))) {
            return yield* fail5(new InvalidType(ast, input, options));
          }
          compileMembers();
          const record3 = input;
          const out = {};
          const state = {
            ast,
            input: record3,
            out,
            issues: void 0,
            options
          };
          const errorsAllOption = options.errors === "all";
          const onExcessPropertyError = options.onExcessProperty === "error";
          const concurrency = options.concurrency === void 0 ? 1 : resolveConcurrency(options.concurrency);
          const indexKeys = indexCount && onExcessPropertyError ? ast.indexSignatures.map((index) => getIndexSignatureKeys(record3, index.parameter, options)) : void 0;
          if (onExcessPropertyError) {
            const coveredKeys = indexKeys ? new Set(expectedKeys) : expectedKeys;
            if (indexKeys) {
              for (const keys of indexKeys) {
                for (const key of keys) coveredKeys.add(key);
              }
            }
            const inputKeys = Reflect.ownKeys(record3);
            for (let i = 0; i < inputKeys.length; i++) {
              const key = inputKeys[i];
              if (!coveredKeys.has(key) && Object.prototype.propertyIsEnumerable.call(record3, key)) {
                const unexpected = new UnexpectedKey(ast, record3[key], options);
                const issue2 = new Pointer([key], unexpected);
                if (errorsAllOption) {
                  if (state.issues) {
                    state.issues.push(issue2);
                  } else {
                    state.issues = [issue2];
                  }
                  continue;
                } else {
                  return yield* fail5(new Composite(ast, [issue2], input, options));
                }
              }
            }
          }
          if (hasProperties) {
            const eff = concurrency === 1 ? parseProperties(state, properties) : parsePropertiesConcurrent(state, properties, {
              concurrency
            });
            if (eff) yield* eff;
          }
          if (indexCount && concurrency === 1) {
            for (let i = 0; i < indexCount; i++) {
              const index = indexes[i];
              const parse = index.is.parameter === string2 ? parseStringIndex : parseIndex;
              const keys = indexKeys?.[i] ?? (index.is.parameter === string2 ? Object.keys(record3) : getIndexSignatureKeys(record3, index.is.parameter, options));
              for (let j = 0; j < keys.length; j++) {
                const eff = parse(state, keys[j], index);
                if (!effectIsExit(eff)) yield* eff;
                else if (eff._tag === "Failure") return yield* eff;
              }
            }
          } else if (parseIndexes) {
            const keyPairs = empty2();
            for (let i = 0; i < indexCount; i++) {
              const index = indexes[i];
              const keys = indexKeys?.[i] ?? (index.is.parameter === string2 ? Object.keys(record3) : getIndexSignatureKeys(record3, index.is.parameter, options));
              for (let j = 0; j < keys.length; j++) {
                keyPairs.push([keys[j], index]);
              }
            }
            const eff = parseIndexes(state, keyPairs, {
              concurrency
            });
            if (eff) yield* eff;
          }
          if (state.issues) {
            return yield* fail5(new Composite(ast, state.issues, input, options));
          }
          return out;
        });
      }, "makeFallback");
      if (indexCount) return makeFallback();
      let fallback;
      const resume = /* @__PURE__ */ __name((state, index, pending2) => {
        const property = properties[index];
        return flatMap3(exit2(pending2), (exit3) => {
          const terminal = stepProperty(state, property, exit3);
          if (terminal) return terminal;
          const done4 = /* @__PURE__ */ __name(() => succeed8(state.out), "done");
          const eff = parseProperties(state, properties.slice(index + 1));
          return eff ? flatMapEager2(eff, done4) : done4();
        });
      }, "resume");
      return (input, options) => {
        if (input === missing) return missingExit;
        if (options.errors === "all" || options.onExcessProperty !== void 0 || options.concurrency !== void 0 && resolveConcurrency(options.concurrency) !== 1) {
          return (fallback ??= makeFallback())(input, options);
        }
        if (!(typeof input === "object" && input !== null && !Array.isArray(input))) {
          return fail5(new InvalidType(ast, input, options));
        }
        const props = compileMembers();
        const record3 = input;
        const out = {};
        const state = {
          ast,
          input: record3,
          out,
          issues: void 0,
          options
        };
        try {
          for (let index = 0; index < props.length; index++) {
            const property = props[index];
            const name = property.name;
            const hasKey = hasPropertySignature(record3, name);
            const value = hasKey ? record3[name] : missing;
            const exit3 = property.parser(value, options);
            if (!effectIsExit(exit3)) {
              return resume(state, index, exit3);
            }
            if (exit3 === sameExit) {
              if (hasKey) assignProperty(out, name, value);
              continue;
            }
            const terminal = stepProperty(state, property, exit3);
            if (terminal) return terminal;
          }
        } catch (error) {
          return die3(error);
        }
        return succeed8(out);
      };
    }
    _rebuild(recur, recurParameter, checks, encodingChecks) {
      const props = mapOrSame(this.propertySignatures, (ps) => {
        const t = recur(ps.type);
        return t === ps.type ? ps : new PropertySignature(ps.name, t);
      });
      const indexes = mapOrSame(this.indexSignatures, (is3) => {
        const p = recurParameter(is3.parameter);
        const t = recur(is3.type);
        return p === is3.parameter && t === is3.type ? is3 : new IndexSignature(p, t);
      });
      return props === this.propertySignatures && indexes === this.indexSignatures && checks === this.checks && encodingChecks === this.encodingChecks ? this : new Objects(props, indexes, this.annotations, checks, void 0, this.context, encodingChecks);
    }
    /** @internal */
    flip(recur) {
      return this._rebuild(recur, recur, this.encodingChecks, this.checks);
    }
    /** @internal */
    recur(recur, recurParameter = recur) {
      return this._rebuild(recur, recurParameter, this.checks, this.encodingChecks);
    }
    /** @internal */
    getExpected() {
      if (this.propertySignatures.length === 0 && this.indexSignatures.length === 0) return "object | array";
      return "object";
    }
  };
  function stepProperty(s, p, exit3) {
    if (exit3._tag === "Failure") {
      return wrapPropertyKeyIssue(s, s.ast, p.name, exit3);
    }
    if (exit3 === sameExit) return;
    const value = exit3[args];
    if (value !== missing) {
      assignProperty(s.out, p.name, value);
      return;
    }
    delete s.out[p.name];
    if (!isOptional(p.type)) {
      const issue2 = new Pointer([p.name], new MissingKey(p.type.context?.annotations));
      if (s.options.errors === "all") {
        if (s.issues) s.issues.push(issue2);
        else s.issues = [issue2];
        return;
      } else {
        return fail4(new Composite(s.ast, [issue2], s.input, s.options));
      }
    }
  }
  __name(stepProperty, "stepProperty");
  var parsePropertiesOptions = {
    onItem(s, p) {
      if (!hasPropertySignature(s.input, p.name)) {
        return p.parser(missing, s.options);
      }
      const value = s.input[p.name];
      assignProperty(s.out, p.name, value);
      return p.parser(value, s.options);
    },
    step: stepProperty
  };
  var parseProperties = /* @__PURE__ */ iterateEager()(parsePropertiesOptions);
  var parsePropertiesConcurrent = /* @__PURE__ */ iterateConcurrent()(parsePropertiesOptions);
  function combineChecks(a, b) {
    if (!a) return b;
    if (!b) return a;
    return [...a, ...b];
  }
  __name(combineChecks, "combineChecks");
  function struct(fields, checks, annotations) {
    return new Objects(Reflect.ownKeys(fields).map((key) => {
      return new PropertySignature(key, fields[key].ast);
    }), [], annotations, checks);
  }
  __name(struct, "struct");
  function getAST(self) {
    return self.ast;
  }
  __name(getAST, "getAST");
  function tuple(elements, checks = void 0) {
    return new Arrays(false, elements.map((e) => e.ast), [], void 0, checks);
  }
  __name(tuple, "tuple");
  function union(members, options, checks) {
    return new Union(members.map(getAST), options, void 0, checks);
  }
  __name(union, "union");
  var toCandidate = /* @__PURE__ */ memoizeIdempotent((ast) => {
    while (true) {
      if (isSuspend(ast)) return unknown;
      const encoding = ast.encoding;
      if (!encoding) {
        return ast.recur?.(toCandidate, identity) ?? ast;
      }
      if (encoding.some((link2) => link2.transformation._tag === "Middleware" && link2.transformation.decode !== identity)) return unknown;
      ast = encoding[encoding.length - 1].to;
    }
  });
  function getCandidateTypes(ast) {
    switch (ast._tag) {
      case "Null":
        return ["null"];
      case "Undefined":
        return ["undefined"];
      case "String":
      case "TemplateLiteral":
        return ["string"];
      case "Number":
        return ["number"];
      case "Boolean":
        return ["boolean"];
      case "Symbol":
      case "UniqueSymbol":
        return ["symbol"];
      case "BigInt":
        return ["bigint"];
      case "Arrays":
        return ["array"];
      case "ObjectKeyword":
        return ["object", "array", "function"];
      case "Objects":
        return ast.propertySignatures.length || ast.indexSignatures.length ? ["object"] : ["string", "number", "boolean", "symbol", "bigint", "object", "array", "function"];
      case "Enum":
        return Array.from(new Set(ast.enums.map(([, v]) => typeof v)));
      case "Literal":
        return [typeof ast.literal];
      case "Union":
        return Array.from(new Set(ast.types.flatMap(getCandidateTypes)));
      default:
        return ["null", "undefined", "string", "number", "boolean", "symbol", "bigint", "object", "array", "function"];
    }
  }
  __name(getCandidateTypes, "getCandidateTypes");
  function collectSentinels(ast) {
    switch (ast._tag) {
      default:
        return [];
      case "Declaration": {
        const s = ast.annotations?.[SENTINELS_ANNOTATION_KEY];
        return Array.isArray(s) ? s : [];
      }
      case "Objects":
        return ast.propertySignatures.flatMap((ps) => {
          const type = ps.type;
          if (!isOptional(type)) {
            if (isLiteral(type)) {
              return [{
                key: ps.name,
                literal: type.literal
              }];
            }
            if (isUniqueSymbol(type)) {
              return [{
                key: ps.name,
                literal: type.symbol
              }];
            }
          }
          return [];
        });
      case "Arrays":
        return ast.elements.flatMap((e, i) => {
          if (!isOptional(e)) {
            if (isLiteral(e)) {
              return [{
                key: i,
                literal: e.literal
              }];
            }
            if (isUniqueSymbol(e)) {
              return [{
                key: i,
                literal: e.symbol
              }];
            }
          }
          return [];
        });
      case "Union": {
        if (ast.types.length === 0) return [];
        const members = ast.types.map((type) => collectSentinels(toCandidate(type)));
        return members[0].filter((s) => members.every((sentinels) => sentinels.some((o) => o.key === s.key && o.literal === s.literal)));
      }
      case "Suspend":
        return collectSentinels(ast.thunk());
    }
  }
  __name(collectSentinels, "collectSentinels");
  var candidateIndexCache = /* @__PURE__ */ new WeakMap();
  var emptyCandidates = /* @__PURE__ */ Object.freeze([]);
  var getRuntimeType = /* @__PURE__ */ __name((input) => input === null ? "null" : Array.isArray(input) ? "array" : typeof input, "getRuntimeType");
  var hasPropertySignature = /* @__PURE__ */ __name((input, key) => key === "__proto__" ? Object.hasOwn(input, key) : key in input, "hasPropertySignature");
  function getCandidateIndex(types) {
    let index = candidateIndexCache.get(types);
    if (index) return index;
    let bySentinel;
    let sentinelCandidateCount = 0;
    let otherwise;
    let literalCandidates;
    let onlyLiterals = true;
    const literalOf = [];
    for (let i = 0; i < types.length; i++) {
      const a = types[i];
      const encoded = toCandidate(a);
      if (isNever2(encoded)) continue;
      if (isLiteral(encoded) || isUniqueSymbol(encoded)) {
        literalCandidates ??= /* @__PURE__ */ new Map();
        const literal2 = isLiteral(encoded) ? encoded.literal : encoded.symbol;
        literalOf[i] = literal2;
        let arr = literalCandidates.get(literal2);
        if (!arr) literalCandidates.set(literal2, arr = []);
        arr.push(i);
      } else {
        onlyLiterals = false;
      }
      const sentinels = collectSentinels(encoded);
      if (sentinels.length) {
        bySentinel ??= /* @__PURE__ */ new Map();
        sentinelCandidateCount++;
        for (const {
          key,
          literal: literal2
        } of sentinels) {
          let entry = bySentinel.get(key);
          if (!entry) bySentinel.set(key, entry = [/* @__PURE__ */ new Map(), /* @__PURE__ */ new Set()]);
          entry[1].add(i);
          let indexes = entry[0].get(literal2);
          if (!indexes) entry[0].set(literal2, indexes = /* @__PURE__ */ new Set());
          indexes.add(i);
        }
      } else {
        otherwise ??= {};
        const candidateTypes = getCandidateTypes(encoded);
        for (const t of candidateTypes) (otherwise[t] ??= []).push(i);
      }
    }
    const fallbacks = {};
    const getFallback = /* @__PURE__ */ __name((type) => fallbacks[type] ??= Object.freeze(otherwise?.[type] ?? emptyCandidates), "getFallback");
    if (onlyLiterals && literalCandidates) {
      literalCandidates.forEach(Object.freeze);
      index = /* @__PURE__ */ __name((input) => literalCandidates.get(input) ?? emptyCandidates, "index");
    } else if (bySentinel?.size === 1 && !otherwise) {
      const [key, [byValue]] = bySentinel.entries().next().value;
      const candidates = /* @__PURE__ */ new Map();
      for (const [literal2, indexes] of byValue) {
        candidates.set(literal2, Object.freeze(Array.from(indexes)));
      }
      const all3 = Object.freeze(types.map((_, i) => i));
      index = /* @__PURE__ */ __name((input, isConstructor) => {
        if (isObjectKeyword(input)) {
          const value = hasPropertySignature(input, key) ? input[key] : void 0;
          if (value !== void 0) return candidates.get(value) ?? emptyCandidates;
          if (isConstructor) return all3;
        }
        return emptyCandidates;
      }, "index");
    } else if (bySentinel) {
      let commonSentinel;
      for (const entry of bySentinel) {
        if ((!commonSentinel || entry[1][0].size > commonSentinel[1][0].size) && entry[1][1].size === sentinelCandidateCount) {
          commonSentinel = entry;
        }
      }
      index = /* @__PURE__ */ __name((input, isConstructor) => {
        const runtimeType = getRuntimeType(input);
        if (!isObjectKeyword(input)) return getFallback(runtimeType);
        const selected = new Set(otherwise?.[runtimeType]);
        let directKey;
        if (commonSentinel) {
          const [key, [byValue]] = commonSentinel;
          const hasKey = hasPropertySignature(input, key);
          const value = hasKey ? input[key] : void 0;
          if (hasKey && (!isConstructor || value !== void 0)) {
            const match6 = byValue.get(value);
            if (!match6) return getFallback(runtimeType);
            for (const i of match6) selected.add(i);
            directKey = key;
          }
        }
        if (directKey === void 0) {
          for (const [key, [byValue, all3]] of bySentinel) {
            const hasKey = hasPropertySignature(input, key);
            const value = hasKey ? input[key] : void 0;
            if (hasKey && (!isConstructor || value !== void 0)) {
              const match6 = byValue.get(value);
              if (match6) {
                for (const i of match6) selected.add(i);
              }
            } else if (isConstructor) {
              for (const i of all3) selected.add(i);
            }
          }
        }
        for (const [key, [byValue, all3]] of bySentinel) {
          if (key === directKey) continue;
          const hasKey = hasPropertySignature(input, key);
          const value = hasKey ? input[key] : void 0;
          if (hasKey && (!isConstructor || value !== void 0)) {
            const match6 = byValue.get(value);
            for (const i of selected) {
              if (all3.has(i) && !match6?.has(i)) selected.delete(i);
            }
          }
        }
        return Array.from(selected).sort((a, b) => a - b);
      }, "index");
    } else {
      index = /* @__PURE__ */ __name((input) => {
        const fallback = getFallback(getRuntimeType(input));
        return literalCandidates ? fallback.filter((i) => literalOf[i] === void 0 || literalOf[i] === input) : fallback;
      }, "index");
    }
    candidateIndexCache.set(types, index);
    return index;
  }
  __name(getCandidateIndex, "getCandidateIndex");
  var Union = class extends ASTNodeImpl {
    static {
      __name(this, "Union");
    }
    _tag = "Union";
    types;
    options;
    encodingChecks;
    constructor(types, options, annotations, checks, encoding, context3, encodingChecks) {
      super(annotations, checks, encoding, context3);
      this.types = types;
      this.options = options;
      this.encodingChecks = encodingChecks;
    }
    /** @internal */
    getParser(compile2, compileField2) {
      const ast = this;
      const isConstructor = compileField2 !== void 0;
      const parsers2 = [];
      const parser = /* @__PURE__ */ __name((i) => parsers2[i] ??= compile2(ast.types[i]), "parser");
      let index;
      return (input, options) => {
        if (input === missing) {
          return missingExit;
        }
        const candidates = (index ??= getCandidateIndex(ast.types))(input, isConstructor);
        if (candidates.length === 0) {
          return fail5(new AnyOf(ast, [], input, options));
        }
        if (candidates.length === 1) {
          const result2 = parser(candidates[0])(input, options);
          if (result2._tag === "Success") return result2;
          return effectIsExit(result2) ? failSingleUnionCandidate(ast, result2.cause, input, options) : catchSingleUnionCandidate(ast, result2, input, options);
        }
        return parseUnionCandidates(ast, parser, candidates, input, options);
      };
    }
    _rebuild(recur, checks, encodingChecks) {
      const types = mapOrSame(this.types, recur);
      return types === this.types && checks === this.checks && encodingChecks === this.encodingChecks ? this : new Union(types, this.options, this.annotations, checks, void 0, this.context, encodingChecks);
    }
    /** @internal */
    recur(recur) {
      return this._rebuild(recur, this.checks, this.encodingChecks);
    }
    /** @internal */
    flip(recur) {
      return this._rebuild(recur, this.encodingChecks, this.checks);
    }
    /** @internal */
    matchPart(s, options) {
      for (const type of this.types) {
        const out = type.matchPart(s, options);
        if (out !== void 0) return out;
      }
      return void 0;
    }
    /** @internal */
    getExpected(getExpected2) {
      const expected = this.annotations?.expected;
      if (typeof expected === "string") return expected;
      if (this.types.length === 0) return "never";
      const types = this.types.map((type) => {
        const encoded = toEncoded(type);
        switch (encoded._tag) {
          case "Arrays": {
            const literals = encoded.elements.filter(isLiteral);
            if (literals.length > 0) {
              return `${formatIsMutable(encoded.isMutable)}[ ${literals.map((e) => getExpected2(e) + formatIsOptional(e.context?.isOptional)).join(", ")}, ... ]`;
            }
            break;
          }
          case "Objects": {
            const literals = encoded.propertySignatures.filter((ps) => isLiteral(ps.type));
            if (literals.length > 0) {
              return `{ ${literals.map((ps) => `${formatIsMutable(ps.type.context?.isMutable)}${formatPropertyKey(ps.name)}${formatIsOptional(ps.type.context?.isOptional)}: ${getExpected2(ps.type)}`).join(", ")}, ... }`;
            }
            break;
          }
        }
        return getExpected2(encoded);
      });
      return Array.from(new Set(types)).join(" | ");
    }
  };
  function failSingleUnionCandidate(ast, cause, input, options) {
    const issue2 = getSchemaIssue(cause);
    if (!issue2) return failCause2(cause);
    return fail4(new AnyOf(ast, [issue2], input, options));
  }
  __name(failSingleUnionCandidate, "failSingleUnionCandidate");
  function catchSingleUnionCandidate(ast, result2, input, options) {
    return catchCause2(result2, (cause) => failSingleUnionCandidate(ast, cause, input, options));
  }
  __name(catchSingleUnionCandidate, "catchSingleUnionCandidate");
  function parseUnionCandidates(ast, parser, candidates, input, options) {
    const state = {
      ast,
      parser,
      input,
      out: void 0,
      successes: ast.options?.mode === "oneOf" ? [] : void 0,
      issues: void 0,
      options
    };
    const eff = parseUnion(state, candidates);
    if (!eff) {
      if (state.out) return state.out;
      return fail5(new AnyOf(ast, state.issues ?? [], input, options));
    }
    if (effectIsExit(eff)) return resumeUnion(eff, state);
    let first = true;
    return suspend2(() => {
      if (!first) return parseUnionCandidates(ast, parser, candidates, input, options);
      first = false;
      return resumeUnion(eff, state);
    });
  }
  __name(parseUnionCandidates, "parseUnionCandidates");
  function resumeUnion(eff, state) {
    return flatMapEager2(eff, (_) => {
      if (state.out === sameExit) return succeed7(state.input);
      if (state.out) return state.out;
      return fail5(new AnyOf(state.ast, state.issues ?? [], state.input, state.options));
    });
  }
  __name(resumeUnion, "resumeUnion");
  var parseUnion = /* @__PURE__ */ iterateEager()({
    onItem(s, i) {
      return s.parser(i)(s.input, s.options);
    },
    step(s, i, exit3) {
      if (exit3._tag === "Failure") {
        const issue2 = getSchemaIssue(exit3.cause);
        if (issue2 === void 0) {
          return exit3;
        }
        if (s.issues) s.issues.push(issue2);
        else s.issues = [issue2];
      } else {
        if (s.out && s.successes) {
          s.successes.push(s.ast.types[i]);
          return fail4(new OneOf(s.ast, s.successes, s.input, s.options));
        }
        s.out = exit3;
        if (s.successes) {
          s.successes.push(s.ast.types[i]);
        } else {
          return void_2;
        }
      }
    }
  });
  var nonFiniteLiterals = /* @__PURE__ */ new Union([/* @__PURE__ */ new Literal("Infinity"), /* @__PURE__ */ new Literal("-Infinity"), /* @__PURE__ */ new Literal("NaN")]);
  function formatIsMutable(isMutable) {
    return isMutable ? "" : "readonly ";
  }
  __name(formatIsMutable, "formatIsMutable");
  function formatIsOptional(isOptional2) {
    return isOptional2 ? "?" : "";
  }
  __name(formatIsOptional, "formatIsOptional");
  var Filter2 = class extends Class {
    static {
      __name(this, "Filter");
    }
    _tag = "Filter";
    run;
    annotations;
    /**
     * Whether the parsing process should be aborted after this check has failed.
     */
    aborted;
    constructor(run2, annotations = void 0, aborted = false) {
      super();
      this.run = run2;
      this.annotations = annotations;
      this.aborted = aborted;
    }
    annotate(annotations) {
      return new Filter2(this.run, {
        ...this.annotations,
        ...annotations
      }, this.aborted);
    }
    abort() {
      return new Filter2(this.run, this.annotations, true);
    }
    and(other, annotations) {
      return new FilterGroup([this, other], annotations);
    }
  };
  var FilterGroup = class extends Class {
    static {
      __name(this, "FilterGroup");
    }
    _tag = "FilterGroup";
    checks;
    annotations;
    constructor(checks, annotations = void 0) {
      super();
      this.checks = checks;
      this.annotations = annotations;
    }
    annotate(annotations) {
      return new FilterGroup(this.checks, {
        ...this.annotations,
        ...annotations
      });
    }
    and(other, annotations) {
      return new FilterGroup([this, other], annotations);
    }
  };
  function makeFilter(filter7, annotations, aborted = false) {
    return new Filter2((input, ast, options) => normalizeFilterOutput(ast, filter7(input, ast, options), input, options), annotations, aborted);
  }
  __name(makeFilter, "makeFilter");
  function isFinite(annotations) {
    return makeFilter((n) => globalThis.Number.isFinite(n), {
      expected: "a finite number",
      representation: {
        id: "effect/schema/isFinite",
        payload: null
      },
      toJsonSchema: /* @__PURE__ */ __name(() => ({
        type: "number"
      }), "toJsonSchema"),
      toCode: /* @__PURE__ */ __name(() => ({
        runtime: "Schema.isFinite()"
      }), "toCode"),
      arbitraryConstraint: {
        number: "finite"
      },
      ...annotations
    });
  }
  __name(isFinite, "isFinite");
  var finite = /* @__PURE__ */ appendChecks(number2, [/* @__PURE__ */ isFinite()]);
  var numberToJson = /* @__PURE__ */ new Link(/* @__PURE__ */ new Union([finite, nonFiniteLiterals]), /* @__PURE__ */ new Transformation(/* @__PURE__ */ Number4(), /* @__PURE__ */ transform((n) => globalThis.Number.isFinite(n) ? n : globalThis.String(n))));
  function isPattern(regExp, annotations) {
    const copy3 = new globalThis.RegExp(regExp);
    const payload = {
      source: copy3.source,
      flags: copy3.flags
    };
    return makeFilter((s) => {
      copy3.lastIndex = 0;
      return copy3.test(s);
    }, {
      expected: `a string matching the RegExp ${payload.source}`,
      representation: {
        id: "effect/schema/isPattern",
        payload
      },
      // Keep the filter exportable so its JSON Schema annotations are preserved
      // when the RegExp constraint itself is omitted.
      toJsonSchema: /* @__PURE__ */ __name(() => [{}, true], "toJsonSchema"),
      arbitraryConstraint: {
        patterns: [payload]
      },
      ...annotations
    });
  }
  __name(isPattern, "isPattern");
  var bodyOwners = /* @__PURE__ */ new WeakMap();
  function copy2(ast, changes2) {
    const out = Object.assign(Object.create(Object.getPrototypeOf(ast)), ast, changes2);
    if (Reflect.ownKeys(changes2).every((key) => key === "context" || key === "encoding")) {
      bodyOwners.set(out, getContextOwner(ast));
    }
    return out;
  }
  __name(copy2, "copy");
  function getContextOwner(ast) {
    const existing = bodyOwners.get(ast);
    if (existing !== void 0) return existing;
    if (ast.encoding === void 0) return ast;
    const owner = Object.assign(Object.create(Object.getPrototypeOf(ast)), ast, {
      encoding: void 0
    });
    bodyOwners.set(ast, owner);
    return owner;
  }
  __name(getContextOwner, "getContextOwner");
  function replaceEncoding(ast, encoding) {
    return ast.encoding === encoding ? ast : copy2(ast, {
      encoding
    });
  }
  __name(replaceEncoding, "replaceEncoding");
  function replaceContext(ast, context3) {
    if (ast.context === context3) {
      return ast;
    }
    const owner = getContextOwner(ast);
    if (owner.context === context3 && owner.encoding === ast.encoding) {
      return owner;
    }
    return copy2(ast, {
      context: context3
    });
  }
  __name(replaceContext, "replaceContext");
  function getLastEncoding(ast) {
    return ast.encoding ? getLastEncoding(ast.encoding[ast.encoding.length - 1].to) : ast;
  }
  __name(getLastEncoding, "getLastEncoding");
  function annotate(ast, annotations) {
    if (ast.checks) {
      const last = ast.checks[ast.checks.length - 1];
      return replaceChecks(ast, append(ast.checks.slice(0, -1), last.annotate(annotations)));
    }
    return copy2(ast, {
      annotations: {
        ...ast.annotations,
        ...annotations
      }
    });
  }
  __name(annotate, "annotate");
  function replaceChecks(ast, checks) {
    if (ast._tag === "Suspend" && checks) {
      throw new Error("Cannot add checks to Suspend");
    }
    if (ast.checks === checks) {
      return ast;
    }
    return copy2(ast, {
      checks
    });
  }
  __name(replaceChecks, "replaceChecks");
  function appendChecks(ast, checks) {
    return replaceChecks(ast, combineChecks(ast.checks, checks));
  }
  __name(appendChecks, "appendChecks");
  function mapLink(link2, f) {
    const to = f(link2.to);
    return to === link2.to ? link2 : new Link(to, link2.transformation);
  }
  __name(mapLink, "mapLink");
  function updateLastLink(encoding, f) {
    const links = encoding;
    const last = links[links.length - 1];
    const out = mapLink(last, f);
    return out === last ? encoding : append(encoding.slice(0, encoding.length - 1), out);
  }
  __name(updateLastLink, "updateLastLink");
  function applyToLastLink(f) {
    return (ast) => ast.encoding ? replaceEncoding(ast, updateLastLink(ast.encoding, f)) : ast;
  }
  __name(applyToLastLink, "applyToLastLink");
  function applyToSelfOrLastLinkEncodingIdempotent(f, options) {
    function out(ast) {
      if (ast.encoding) {
        const last = ast.encoding[ast.encoding.length - 1];
        return options?.stopAt?.(last) ? ast : replaceEncoding(ast, updateLastLink(ast.encoding, out));
      }
      return f(ast);
    }
    __name(out, "out");
    return memoizeIdempotent(out);
  }
  __name(applyToSelfOrLastLinkEncodingIdempotent, "applyToSelfOrLastLinkEncodingIdempotent");
  function appendTransformation(from, transformation, to) {
    const link2 = new Link(from, transformation);
    return replaceEncoding(to, to.encoding ? [...to.encoding, link2] : [link2]);
  }
  __name(appendTransformation, "appendTransformation");
  function mapOrSame(as3, f) {
    let out;
    for (let i = 0; i < as3.length; i++) {
      const a = as3[i];
      const fa = f(a);
      if (out) {
        out[i] = fa;
      } else if (fa !== a) {
        out = new Array(as3.length);
        for (let j = 0; j < i; j++) {
          out[j] = as3[j];
        }
        out[i] = fa;
      }
    }
    return out ?? as3;
  }
  __name(mapOrSame, "mapOrSame");
  function annotateKey(ast, annotations) {
    const context3 = ast.context ? new Context(ast.context.isOptional, ast.context.isMutable, ast.context.constructorDefault, {
      ...ast.context.annotations,
      ...annotations
    }) : new Context(false, false, void 0, annotations);
    return replaceContext(ast, context3);
  }
  __name(annotateKey, "annotateKey");
  var optionalKey = /* @__PURE__ */ memoizeIdempotent((ast) => {
    const context3 = ast.context ? ast.context.isOptional === false ? new Context(true, ast.context.isMutable, ast.context.constructorDefault, ast.context.annotations) : ast.context : new Context(true, false);
    return optionalKeyLastLink(replaceContext(ast, context3));
  });
  var optionalKeyLastLink = /* @__PURE__ */ applyToLastLink(optionalKey);
  function withConstructorDefault(ast, defaultValue) {
    const context3 = ast.context ? new Context(ast.context.isOptional, ast.context.isMutable, defaultValue, ast.context.annotations) : new Context(false, false, defaultValue);
    return replaceContext(ast, context3);
  }
  __name(withConstructorDefault, "withConstructorDefault");
  function decodeTo(from, to, transformation) {
    return appendTransformation(from, transformation, to);
  }
  __name(decodeTo, "decodeTo");
  function parseParameter(ast) {
    const literals = [];
    const parameters = [];
    function go(ast2) {
      switch (ast2._tag) {
        case "Literal":
          if (isPropertyKey(ast2.literal) && !literals.includes(ast2.literal)) {
            literals.push(ast2.literal);
          }
          return;
        case "UniqueSymbol":
          if (!literals.includes(ast2.symbol)) {
            literals.push(ast2.symbol);
          }
          return;
        case "Never":
          return;
        case "Union":
          for (let i = 0; i < ast2.types.length; i++) {
            go(ast2.types[i]);
          }
          return;
        default:
          parameters.push(ast2);
      }
    }
    __name(go, "go");
    go(ast);
    return {
      literals,
      parameters
    };
  }
  __name(parseParameter, "parseParameter");
  function record(key, value) {
    const {
      literals,
      parameters: indexSignatures
    } = parseParameter(key);
    return new Objects(literals.map((literal2) => new PropertySignature(literal2, value)), indexSignatures.map((parameter) => new IndexSignature(parameter, value)));
  }
  __name(record, "record");
  function isOptional(ast) {
    return ast.context?.isOptional ?? false;
  }
  __name(isOptional, "isOptional");
  function isStructuralCheck(check) {
    return check.annotations?.[STRUCTURAL_ANNOTATION_KEY] === true || check._tag === "FilterGroup" && check.checks.every(isStructuralCheck);
  }
  __name(isStructuralCheck, "isStructuralCheck");
  function extractStructuralChecks(checks) {
    function extract(check) {
      if (isStructuralCheck(check)) return [check];
      return check._tag === "FilterGroup" ? check.checks.flatMap(extract) : [];
    }
    __name(extract, "extract");
    const out = checks.flatMap(extract);
    return isArrayNonEmpty2(out) ? out : void 0;
  }
  __name(extractStructuralChecks, "extractStructuralChecks");
  function canPreserveEncodingChecks(ast) {
    let preserve = true;
    function visit(child2) {
      preserve = preserve && !child2.encoding && !isSuspend(child2);
      if (preserve && "recur" in child2) child2.recur(visit);
      return child2;
    }
    __name(visit, "visit");
    if ("recur" in ast) ast.recur(visit);
    return preserve;
  }
  __name(canPreserveEncodingChecks, "canPreserveEncodingChecks");
  var toType = /* @__PURE__ */ memoizeIdempotent((ast) => {
    const owner = getContextOwner(ast);
    if (owner !== ast) {
      const type2 = toType(owner);
      return type2 === owner && ast.encoding === void 0 ? ast : replaceContext(type2, ast.context);
    }
    const type = "recur" in ast ? ast.recur(toType) : ast;
    if ("encodingChecks" in type && type.encodingChecks) {
      const checks = canPreserveEncodingChecks(ast) ? type.encodingChecks : isArrays(type) || isObjects(type) || isDeclaration(type) && type.typeParameters.length > 0 ? extractStructuralChecks(type.encodingChecks) : void 0;
      return copy2(type, {
        encodingChecks: void 0,
        checks: combineChecks(type.checks, checks)
      });
    }
    return type;
  });
  var toEncoded = /* @__PURE__ */ memoizeIdempotent((ast) => {
    return toType(flip2(ast));
  });
  function flipEncoding(ast, encoding) {
    const links = encoding;
    const len = links.length;
    const last = links[len - 1];
    const ls = [new Link(flip2(replaceEncoding(ast, void 0)), links[0].transformation.flip())];
    for (let i = 1; i < len; i++) {
      ls.unshift(new Link(flip2(links[i - 1].to), links[i].transformation.flip()));
    }
    const to = flip2(last.to);
    if (to.encoding) {
      return replaceEncoding(to, [...to.encoding, ...ls]);
    } else {
      return replaceEncoding(to, ls);
    }
  }
  __name(flipEncoding, "flipEncoding");
  var flip2 = /* @__PURE__ */ memoize((ast) => {
    if (ast.encoding) {
      return flipEncoding(ast, ast.encoding);
    }
    const owner = getContextOwner(ast);
    if (owner !== ast) {
      const flipped = flip2(owner);
      return flipped === owner ? ast : replaceContext(flipped, ast.context);
    }
    return "flip" in ast ? ast.flip(flip2) : "recur" in ast ? ast.recur(flip2) : ast;
  });
  function containsUndefined(ast) {
    switch (ast._tag) {
      case "Undefined":
        return true;
      case "Union":
        return ast.types.some(containsUndefined);
      default:
        return false;
    }
  }
  __name(containsUndefined, "containsUndefined");
  function fromConst(ast, value) {
    const succeed10 = value === 0 ? sameExit : succeed8(value);
    return (input, options) => {
      if (input === missing) return missingExit;
      if (input === value) return succeed10;
      return fail5(new InvalidType(ast, input, options));
    };
  }
  __name(fromConst, "fromConst");
  function fromRefinement(ast, refinement) {
    return (input, options) => {
      if (input === missing) return missingExit;
      if (refinement(input)) return sameExit;
      return fail5(new InvalidType(ast, input, options));
    };
  }
  __name(fromRefinement, "fromRefinement");
  var parameterFromPropertyKey = /* @__PURE__ */ applyToSelfOrLastLinkEncodingIdempotent((ast) => {
    switch (ast._tag) {
      default:
        return ast;
      case "Number":
        return ast.toCodecStringTree();
      case "Union":
        return ast.recur(parameterFromPropertyKey);
    }
  });
  var isStringFiniteRegExp = /* @__PURE__ */ new globalThis.RegExp(`^${FINITE_PATTERN}$`);
  var isStringNumberRegExp = /* @__PURE__ */ new globalThis.RegExp(`^(?:${FINITE_PATTERN}|Infinity|-Infinity|NaN)$`);
  function isStringFinite(annotations) {
    return isPattern(isStringFiniteRegExp, {
      expected: "a string representing a finite number",
      representation: {
        id: "effect/schema/isStringFinite",
        payload: null
      },
      toJsonSchema: /* @__PURE__ */ __name(() => ({
        pattern: isStringFiniteRegExp.source
      }), "toJsonSchema"),
      ...annotations
    });
  }
  __name(isStringFinite, "isStringFinite");
  var finiteString = /* @__PURE__ */ appendChecks(string2, [/* @__PURE__ */ isStringFinite()]);
  var finiteToString = /* @__PURE__ */ new Link(finiteString, numberFromString);
  var numberToString = /* @__PURE__ */ new Link(/* @__PURE__ */ new Union([finiteString, nonFiniteLiterals]), numberFromString);
  var BIGINT_PATTERN = "-?\\d+";
  var isStringBigIntRegExp = /* @__PURE__ */ new globalThis.RegExp(`^${BIGINT_PATTERN}$`);
  var REGEXP_PATTERN = "Symbol\\(([\\s\\S]*)\\)";
  var isStringSymbolRegExp = /* @__PURE__ */ new globalThis.RegExp(`^${REGEXP_PATTERN}$`);
  function collectIssues(checks, value, issues, ast, options) {
    for (let i = 0; i < checks.length; i++) {
      const check = checks[i];
      if (check._tag === "FilterGroup") {
        issues = collectIssues(check.checks, value, issues, ast, options);
        if (issues && (options.errors !== "all" || issues[issues.length - 1].filter.aborted)) {
          return issues;
        }
      } else {
        const issue2 = check.run(value, ast, options);
        if (issue2) {
          const filter7 = new Filter(check, issue2, value, options);
          if (issues) issues.push(filter7);
          else issues = [filter7];
          if (options.errors !== "all" || check.aborted) {
            return issues;
          }
        }
      }
    }
    return issues;
  }
  __name(collectIssues, "collectIssues");
  function getConstructorDescriptor(ast) {
    if (!isDeclaration(ast)) return void 0;
    const getDescriptor = ast.annotations?.[CONSTRUCTOR_ANNOTATION_KEY];
    return isFunction(getDescriptor) ? getDescriptor(ast.typeParameters) : void 0;
  }
  __name(getConstructorDescriptor, "getConstructorDescriptor");
  function isJsonLeaf(u) {
    return u === null || typeof u === "string" || typeof u === "boolean" || typeof u === "number" && globalThis.Number.isFinite(u);
  }
  __name(isJsonLeaf, "isJsonLeaf");
  function isStringTreeLeaf(u) {
    return u === void 0 || typeof u === "string";
  }
  __name(isStringTreeLeaf, "isStringTreeLeaf");
  function isTree(u, isLeaf) {
    const cache2 = /* @__PURE__ */ new WeakMap();
    const stack = [];
    outer: while (true) {
      if (typeof u !== "object" || u === null) {
        if (!isLeaf(u)) {
          return false;
        }
      } else {
        const value = u;
        const cached2 = cache2.get(value);
        if (cached2 === false) {
          return false;
        }
        if (cached2 === void 0) {
          const isArray2 = Array.isArray(value);
          if (!isArray2) {
            const prototype = Object.getPrototypeOf(value);
            if (prototype !== null && prototype !== Object.prototype && Object.getPrototypeOf(prototype) !== null) {
              return false;
            }
          }
          cache2.set(value, false);
          stack.push({
            value,
            keys: isArray2 ? value.length : Object.keys(value),
            index: 0
          });
        }
      }
      while (stack.length > 0) {
        const frame = stack[stack.length - 1];
        const keys = frame.keys;
        if (typeof keys === "number") {
          if (frame.index < keys) {
            u = frame.value[frame.index++];
            continue outer;
          }
        } else if (frame.index < keys.length) {
          u = frame.value[keys[frame.index++]];
          continue outer;
        }
        cache2.set(frame.value, true);
        stack.pop();
      }
      return true;
    }
  }
  __name(isTree, "isTree");
  function isJson(u) {
    return isTree(u, isJsonLeaf);
  }
  __name(isJson, "isJson");
  var Json = /* @__PURE__ */ new Declaration([], () => (input, ast, options) => isJson(input) ? sameExit : fail5(new InvalidType(ast, input, options)), {
    representation: {
      id: "effect/schema/Json",
      payload: null
    },
    expected: "JSON value",
    toCodecJson: /* @__PURE__ */ __name(() => void 0, "toCodecJson"),
    toCodecStringTree: /* @__PURE__ */ __name(() => unknownToStringTree, "toCodecStringTree")
  });
  function isStringTree(u) {
    return isTree(u, isStringTreeLeaf);
  }
  __name(isStringTree, "isStringTree");
  var StringTree = /* @__PURE__ */ new Declaration([], () => (input, ast, options) => isStringTree(input) ? sameExit : fail5(new InvalidType(ast, input, options)), {
    expected: "StringTree",
    toCodecStringTree: /* @__PURE__ */ __name(() => void 0, "toCodecStringTree")
  });
  var unknownToStringTree = /* @__PURE__ */ new Link(StringTree, /* @__PURE__ */ passthrough3());

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schema/interpreter.js
  var flatMapTransformation = /* @__PURE__ */ __name((result2, current, f) => result2 === sameExit ? f(current) : flatMapEager2(result2, f), "flatMapTransformation");
  function compileTransformation(transformation) {
    if (transformation._tag === "Middleware") {
      return (result2, current, options) => {
        const transformed = result2 === sameExit ? transformation.decode(succeed8(toOption(current)), options) : transformation.decode(mapEager2(result2, toOption), options);
        return fromOptionalEffect(transformed);
      };
    }
    const getter = transformation.decode;
    switch (getter._tag) {
      case "Passthrough":
        return (result2, current) => result2 === sameExit ? succeed8(current) : result2;
      case "Transform": {
        const transform3 = /* @__PURE__ */ __name((value) => value === missing ? missingExit : succeed8(getter.transform(value)), "transform");
        return (result2, current) => flatMapTransformation(result2, current, transform3);
      }
      case "TransformOptional": {
        const transform3 = /* @__PURE__ */ __name((value) => fromOptionExit(getter.transform(toOption(value))), "transform");
        return (result2, current) => flatMapTransformation(result2, current, transform3);
      }
      case "TransformEffect":
        return (result2, current, options) => flatMapTransformation(result2, current, (value) => value === missing ? missingExit : getter.transform(value, options));
      case "TransformOptionalEffect":
        return (result2, current, options) => flatMapTransformation(result2, current, (value) => fromOptionalEffect(getter.transform(toOption(value), options)));
    }
  }
  __name(compileTransformation, "compileTransformation");
  var fromOptionalEffect = /* @__PURE__ */ __name((effect2) => flatMapEager2(effect2, fromOptionExit), "fromOptionalEffect");
  var wrapEncoding = /* @__PURE__ */ __name((ast, input, options, effect2) => catchCause2(effect2, (cause) => failCauseSync2(() => map4(cause, (issue2) => new Encoding(ast, issue2, input, options)))), "wrapEncoding");
  function makeConstructorParser(descriptor, compile2) {
    const transform3 = compileTransformation(descriptor.link.transformation);
    let sourceParser;
    return (input, options) => {
      if (input === missing) return missingExit;
      if (descriptor.isConstructed(input)) return sameExit;
      const result2 = (sourceParser ??= compile2(descriptor.link.to))(input, options);
      return transform3(result2, input, options);
    };
  }
  __name(makeConstructorParser, "makeConstructorParser");
  function withDefault(ast, parser) {
    const defaultValue = ast.context.constructorDefault;
    return (input, options) => {
      if (input !== missing && input !== void 0) return parser(input, options);
      const result2 = defaultValue;
      if (effectIsExit(result2) && result2._tag === "Success") {
        const local = parser(result2[args], options);
        return local === sameExit ? result2 : local;
      }
      return flatMapEager2(wrapEncoding(ast, input, options, result2), (value) => {
        const local = parser(value, options);
        return local === sameExit ? succeed8(value) : local;
      });
    };
  }
  __name(withDefault, "withDefault");
  function compileField(ast, compile2) {
    const parser = compile2(ast);
    return ast.context?.constructorDefault === void 0 ? parser : withDefault(ast, parser);
  }
  __name(compileField, "compileField");
  function compile(ast, compile2, compileField2, base, specialize) {
    if (ast._tag === "Declaration") {
      for (const parameter of ast.typeParameters) compile2(parameter);
    }
    const descriptor = compileField2 ? getConstructorDescriptor(ast) : void 0;
    const parser = descriptor ? makeConstructorParser(descriptor, compile2) : base ?? ast.getParser(compile2, compileField2);
    const checks = ast.checks;
    const links = ast.encoding;
    const transformations = links?.map((link2) => compileTransformation(link2.transformation));
    const encodingChecks = ast.encodingChecks;
    if (!links && !checks && !encodingChecks) {
      return parser;
    }
    let encodingParsers;
    const parseChecks = /* @__PURE__ */ __name((input, options) => {
      let result2 = parser(input, options);
      if (encodingChecks && !options.disableChecks) {
        if (effectIsExit(result2)) {
          if (result2._tag === "Success") {
            const output = result2 === sameExit ? input : result2[args];
            if (input !== missing && output !== missing) {
              const issues = collectIssues(encodingChecks, input, void 0, ast, options);
              if (issues) {
                result2 = fail5(new Composite(ast, issues, input, options));
              }
            }
          }
        } else {
          result2 = flatMap3(result2, (value) => {
            if (input !== missing && value !== missing) {
              const issues = collectIssues(encodingChecks, input, void 0, ast, options);
              if (issues) {
                return fail5(new Composite(ast, issues, input, options));
              }
            }
            return succeed7(value);
          });
        }
      }
      if (checks && !options.disableChecks) {
        if (effectIsExit(result2)) {
          if (result2._tag === "Success") {
            const value = result2 === sameExit ? input : result2[args];
            if (value === missing) return result2;
            const issues = collectIssues(checks, value, void 0, ast, options);
            if (issues) {
              result2 = fail5(new Composite(ast, issues, value, options));
            }
          }
        } else {
          result2 = flatMap3(result2, (value) => {
            if (value !== missing) {
              const issues = collectIssues(checks, value, void 0, ast, options);
              if (issues) {
                return fail5(new Composite(ast, issues, value, options));
              }
            }
            return succeed7(value);
          });
        }
      }
      return result2;
    }, "parseChecks");
    const parseLocal = specialize === void 0 ? parseChecks : specialize(parseChecks);
    if (!links) {
      return parseLocal;
    }
    return (input, options) => {
      const parsers2 = encodingParsers ??= links.map((link2) => compile2(link2.to));
      let current = input;
      let result2 = parsers2[parsers2.length - 1](input, options);
      for (let i = links.length - 1; i >= 0; i--) {
        result2 = transformations[i](result2, current, options);
        if (i !== 0) {
          const next = parsers2[i - 1];
          if (result2._tag === "Success") {
            current = result2[args];
            result2 = next(current, options);
          } else {
            result2 = flatMapEager2(result2, (value) => {
              const nextResult = next(value, options);
              return nextResult === sameExit ? succeed8(value) : nextResult;
            });
          }
        }
      }
      if (result2._tag === "Success") {
        const value = result2[args];
        const local = parseLocal(value, options);
        return local === sameExit ? result2 : local;
      }
      result2 = wrapEncoding(ast, input, options, result2);
      return flatMapEager2(result2, (value) => {
        const local = parseLocal(value, options);
        return local === sameExit ? succeed8(value) : local;
      });
    };
  }
  __name(compile, "compile");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schema/compilerRegistry.js
  var invalid2 = /* @__PURE__ */ Symbol();
  var cache = /* @__PURE__ */ new WeakMap();
  var compiler;
  var compilerAdaptersEnabled = false;
  var decodeChild = /* @__PURE__ */ __name((ast) => compilerAdaptersEnabled ? lazyParser(resolve2, ast, "parser") : resolve2(ast).parser, "decodeChild");
  var makeChild = /* @__PURE__ */ __name((ast) => compilerAdaptersEnabled ? lazyParser(resolve2, ast, "makeEffect") : resolve2(ast).makeEffect, "makeChild");
  var makeField = /* @__PURE__ */ __name((ast) => compileField(ast, makeChild), "makeField");
  var InterpretedEntry = class {
    static {
      __name(this, "InterpretedEntry");
    }
    ast;
    constructor(ast) {
      this.ast = ast;
    }
    get decodeEffect() {
      return this.cachedDecodeEffect ??= compile(this.ast, decodeChild);
    }
    get parser() {
      return this.decodeEffect;
    }
    get makeEffect() {
      return this.cachedMakeEffect ??= compile(this.ast, makeChild, makeField);
    }
  };
  var CompilerEntry = class extends InterpretedEntry {
    static {
      __name(this, "CompilerEntry");
    }
    compiled;
    resolve;
    constructor(ast, compiled, resolve3) {
      super(ast);
      this.compiled = compiled;
      this.resolve = resolve3;
    }
    save(key, value) {
      Object.defineProperty(this, key, {
        value
      });
      return value;
    }
    operation(key) {
      const compiled = this.compiled;
      return typeof compiled === "function" ? compiled(this.ast, this.resolve, key) : compiled?.[key];
    }
    get is() {
      return this.save("is", this.operation("is"));
    }
    get decode() {
      return this.save("decode", this.operation("decode"));
    }
    get make() {
      return this.save("make", this.operation("make"));
    }
    get decodeEffect() {
      return this.save("decodeEffect", this.operation("decodeEffect") ?? compile(this.ast, (ast) => lazyParser(this.resolve, ast, "parser")));
    }
    get parser() {
      const decode3 = this.decode;
      return decode3 === void 0 ? this.decodeEffect : this.save("parser", withDecode(decode3, () => this.decodeEffect));
    }
    get makeEffect() {
      const makeEffect2 = this.operation("makeEffect");
      if (makeEffect2 !== void 0) return this.save("makeEffect", makeEffect2);
      const child2 = /* @__PURE__ */ __name((ast) => lazyParser(this.resolve, ast, "makeEffect"), "child");
      return this.save("makeEffect", compile(this.ast, child2, (ast) => compileField(ast, child2)));
    }
  };
  function withDecode(fastDecode, decodeEffect2) {
    let detailed;
    return (input, options) => {
      if (input !== missing) {
        try {
          const value = fastDecode(input, options);
          if (value !== invalid2) return value === input ? sameExit : succeed8(value);
        } catch (error) {
          return die3(error);
        }
      }
      return (detailed ??= decodeEffect2())(input, options);
    };
  }
  __name(withDecode, "withDecode");
  function lazyParser(resolve3, ast, operation) {
    const entry = resolve3(ast);
    if (entry.compiled === void 0 || Object.hasOwn(entry, operation)) {
      return entry[operation];
    }
    let parser;
    return (input, options) => (parser ??= entry[operation])(input, options);
  }
  __name(lazyParser, "lazyParser");
  function resolve2(ast) {
    const cached2 = cache.get(ast);
    if (cached2 !== void 0) return cached2;
    const entry = compiler === void 0 ? new InterpretedEntry(ast) : new CompilerEntry(ast, compiler(ast, resolve2), resolve2);
    cache.set(ast, entry);
    return entry;
  }
  __name(resolve2, "resolve");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/SchemaParser.js
  function makeEffect(schema) {
    const ast = schema.ast;
    let parser;
    return (input, options) => {
      return (parser ??= runWithCompiler(constructorCompiler, toType(ast)))(input, options?.disableChecks ? options?.parseOptions ? {
        ...options.parseOptions,
        disableChecks: true
      } : {
        disableChecks: true
      } : options?.parseOptions);
    };
  }
  __name(makeEffect, "makeEffect");
  function makeOption(schema) {
    const parser = makeEffect(schema);
    return (input, options) => {
      const exit3 = runSyncExit2(parser(input, options));
      if (isSuccess3(exit3)) {
        return some2(exit3.value);
      }
      getSchemaIssueOrThrow(exit3.cause, "Option adapter can only return none for schema issues");
      return none2();
    };
  }
  __name(makeOption, "makeOption");
  function make8(schema) {
    return makeConstructorSync(toType(schema.ast));
  }
  __name(make8, "make");
  function is(schema) {
    return _is(schema.ast);
  }
  __name(is, "is");
  function makeIs(ast) {
    if (!compilerAdaptersEnabled) {
      const parser2 = asExit(run(ast));
      return (input) => {
        const exit3 = parser2(input, defaultParseOptions);
        if (isSuccess3(exit3)) return true;
        getSchemaIssueOrThrow(exit3.cause, "Type guard adapter can only return false for schema issues");
        return false;
      };
    }
    const entry = resolve2(ast);
    const guard = entry.is;
    if (guard !== void 0) {
      return (input) => {
        try {
          return guard(input, defaultParseOptions);
        } catch (error) {
          getSchemaIssueOrThrow(die2(error), "Type guard adapter can only return false for schema issues");
          return false;
        }
      };
    }
    const parser = entry.parser;
    return (input) => {
      const exit3 = runSyncExit2(parserResult(parser(input, defaultParseOptions), input));
      if (isSuccess3(exit3)) return true;
      getSchemaIssueOrThrow(exit3.cause, "Type guard adapter can only return false for schema issues");
      return false;
    };
  }
  __name(makeIs, "makeIs");
  function _is(ast) {
    const typeAST = toType(ast);
    let guard = /* @__PURE__ */ __name((input) => {
      guard = makeIs(typeAST);
      return guard(input);
    }, "guard");
    return (input) => {
      return guard(input);
    };
  }
  __name(_is, "_is");
  function decodeUnknownEffect(schema, options) {
    const parser = run(schema.ast);
    return options === void 0 ? parser : (input, overrideOptions) => parser(input, mergeParseOptions(options, overrideOptions));
  }
  __name(decodeUnknownEffect, "decodeUnknownEffect");
  function decodeUnknownOption(schema, options) {
    return asOption(decodeUnknownEffect(schema, options));
  }
  __name(decodeUnknownOption, "decodeUnknownOption");
  function decodeUnknownResult(schema, options) {
    return asResult(decodeUnknownEffect(schema, options));
  }
  __name(decodeUnknownResult, "decodeUnknownResult");
  var mergeParseOptions = /* @__PURE__ */ __name((options, overrideOptions) => overrideOptions ? {
    ...options,
    ...overrideOptions
  } : options, "mergeParseOptions");
  var getValue = /* @__PURE__ */ __name((value) => {
    if (value === missing) {
      return fail5(new InvalidValue());
    }
    return succeed7(value);
  }, "getValue");
  function run(ast) {
    return runWithCompiler(normalCompiler, ast);
  }
  __name(run, "run");
  function parserResult(result2, input) {
    if (result2 === sameExit) {
      return succeed7(input);
    }
    if (!effectIsExit(result2)) {
      return flatMapEager2(result2, getValue);
    }
    return result2[args] === missing ? getValue(missing) : result2;
  }
  __name(parserResult, "parserResult");
  function runWithCompiler(compiler2, ast) {
    let parser;
    return (input, options) => {
      const result2 = (parser ??= compiler2(ast))(input, options ?? defaultParseOptions);
      if (result2 === sameExit) {
        return succeed7(input);
      }
      if (!effectIsExit(result2)) {
        return flatMapEager2(result2, getValue);
      }
      return result2[args] === missing ? getValue(missing) : result2;
    };
  }
  __name(runWithCompiler, "runWithCompiler");
  function asExit(parser) {
    return (input, options) => runSyncExit2(parser(input, options));
  }
  __name(asExit, "asExit");
  function asOption(parser) {
    const parserExit = asExit(parser);
    return (input, options) => {
      const exit3 = parserExit(input, options);
      if (isSuccess3(exit3)) {
        return some2(exit3.value);
      }
      getSchemaIssueOrThrow(exit3.cause, "Option adapter can only return none for schema issues");
      return none2();
    };
  }
  __name(asOption, "asOption");
  function asResult(parser) {
    const parserExit = asExit(parser);
    return (input, options) => {
      const exit3 = parserExit(input, options);
      if (isSuccess3(exit3)) {
        return succeed2(exit3.value);
      }
      return fail2(getSchemaIssueOrThrow(exit3.cause, "Result adapter can only return schema issues"));
    };
  }
  __name(asResult, "asResult");
  function runSync3(effect2, message2) {
    const exit3 = runSyncExit2(effect2);
    if (isSuccess3(exit3)) {
      return exit3.value;
    }
    const issue2 = getSchemaIssueOrThrow(exit3.cause, message2);
    throw new Error("Schema validation failed", {
      cause: issue2
    });
  }
  __name(runSync3, "runSync");
  function makeConstructorSync(ast) {
    let entry;
    let parser;
    return (input, options) => {
      entry ??= resolve2(ast);
      const parseOptions = options?.disableChecks ? options.parseOptions ? {
        ...options.parseOptions,
        disableChecks: true
      } : {
        disableChecks: true
      } : options?.parseOptions ?? defaultParseOptions;
      const make18 = entry.make;
      if (make18 !== void 0 && input !== missing) {
        let output;
        try {
          output = make18(input, parseOptions);
        } catch (error) {
          getSchemaIssueOrThrow(die2(error), "Constructor adapter can only throw schema issues");
          throw error;
        }
        if (output !== invalid2 && output !== missing) return output;
      }
      const result2 = (parser ??= entry.makeEffect)(input, parseOptions);
      return runSync3(parserResult(result2, input), "Constructor adapter can only throw schema issues");
    };
  }
  __name(makeConstructorSync, "makeConstructorSync");
  var normalCompiler = /* @__PURE__ */ __name((ast) => resolve2(ast).parser, "normalCompiler");
  var constructorCompiler = /* @__PURE__ */ __name((ast) => resolve2(ast).makeEffect, "constructorCompiler");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schema/make.js
  var TypeId12 = "~effect/Schema/Schema";
  var RebuildOptions = /* @__PURE__ */ Symbol();
  var SchemaProto = {
    [TypeId12]: TypeId12,
    get make() {
      const value = make8(this);
      Object.defineProperty(this, "make", {
        value,
        enumerable: true
      });
      return value;
    },
    get makeEffect() {
      const value = makeEffect(this);
      Object.defineProperty(this, "makeEffect", {
        value,
        enumerable: true
      });
      return value;
    },
    get makeOption() {
      const value = makeOption(this);
      Object.defineProperty(this, "makeOption", {
        value,
        enumerable: true
      });
      return value;
    },
    pipe() {
      return pipeArguments(this, arguments);
    },
    annotate(annotations) {
      return this.rebuild(annotate(this.ast, annotations));
    },
    annotateKey(annotations) {
      return this.rebuild(annotateKey(this.ast, annotations));
    },
    check(...checks) {
      return this.rebuild(appendChecks(this.ast, checks));
    },
    rebuild(ast) {
      return make9(ast, this[RebuildOptions]);
    }
  };
  function make9(ast, options) {
    function Schema() {
    }
    __name(Schema, "Schema");
    const self = Object.setPrototypeOf(Schema, SchemaProto);
    if (options && (Object.hasOwn(options, "name") || Object.hasOwn(options, "length") || Object.hasOwn(options, "__proto__"))) {
      Object.defineProperties(self, Object.getOwnPropertyDescriptors({
        ...options
      }));
    } else {
      Object.assign(self, options);
    }
    self[RebuildOptions] = options;
    self.ast = ast;
    return self;
  }
  __name(make9, "make");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Struct.js
  var lambda = /* @__PURE__ */ __name((f) => f, "lambda");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/schemaError.js
  var SchemaErrorTypeId = "~effect/Schema/SchemaError";

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Schema.js
  var TypeId13 = TypeId12;
  function declareConstructor() {
    return (typeParameters, run2, annotations) => {
      return make10(new Declaration(typeParameters.map(getAST), (typeParameters2) => run2(typeParameters2.map((ast) => make10(ast))), annotations));
    };
  }
  __name(declareConstructor, "declareConstructor");
  function declare(is3, annotations) {
    return declareConstructor()([], () => (input, ast, options) => is3(input) ? succeed7(input) : fail5(new InvalidType(ast, input, options)), annotations);
  }
  __name(declare, "declare");
  var SchemaError = class extends (/* @__PURE__ */ TaggedError2("SchemaError")) {
    static {
      __name(this, "SchemaError");
    }
    [SchemaErrorTypeId] = SchemaErrorTypeId;
    constructor(issue2) {
      const stackTraceLimit = getStackTraceLimit();
      setStackTraceLimit(0);
      try {
        super({
          issue: issue2
        });
      } finally {
        setStackTraceLimit(stackTraceLimit);
      }
    }
    get message() {
      return defaultFormatter(this.issue);
    }
    toJSON() {
      return {
        _tag: this._tag,
        message: this.message
      };
    }
    [NodeInspectSymbol]() {
      return this.toJSON();
    }
    toString() {
      return `SchemaError(${this.message})`;
    }
  };
  function fromIssueEffect(self) {
    if (effectIsExit(self)) {
      return fromIssueExit(self);
    }
    return catchCause2(self, (cause) => failCauseSync2(() => map4(cause, (issue2) => new SchemaError(issue2))));
  }
  __name(fromIssueEffect, "fromIssueEffect");
  function fromIssueExit(exit3) {
    return isSuccess3(exit3) ? exit3 : failCause2(map4(exit3.cause, (issue2) => new SchemaError(issue2)));
  }
  __name(fromIssueExit, "fromIssueExit");
  var is2 = is;
  function decodeUnknownEffect2(schema, options) {
    const parser = decodeUnknownEffect(schema, options);
    return (input, options2) => {
      return fromIssueEffect(parser(input, options2));
    };
  }
  __name(decodeUnknownEffect2, "decodeUnknownEffect");
  var decodeUnknownOption2 = decodeUnknownOption;
  var make10 = make9;
  function isSchema(u) {
    return hasProperty(u, TypeId13) && u[TypeId13] === TypeId13;
  }
  __name(isSchema, "isSchema");
  var optionalKey2 = /* @__PURE__ */ lambda((schema) => make10(optionalKey(schema.ast), {
    schema
  }));
  function Literal2(literal2) {
    const out = make10(new Literal(literal2), {
      literal: literal2,
      transform(to) {
        return out.pipe(decodeTo2(Literal2(to), {
          decode: transform(() => to),
          encode: transform(() => literal2)
        }));
      }
    });
    return out;
  }
  __name(Literal2, "Literal");
  var Never2 = /* @__PURE__ */ make10(never3);
  var Unknown2 = /* @__PURE__ */ make10(unknown);
  var Null2 = /* @__PURE__ */ make10(null_);
  var Undefined2 = /* @__PURE__ */ make10(undefined_2);
  var String4 = /* @__PURE__ */ make10(string2);
  var Number6 = /* @__PURE__ */ make10(number2);
  var Boolean3 = /* @__PURE__ */ make10(boolean);
  var Void2 = /* @__PURE__ */ make10(void_4);
  function makeStruct(ast, fields) {
    return make10(ast, {
      fields,
      mapFields(f, options) {
        const fields2 = f(this.fields);
        return makeStruct(struct(fields2, options?.unsafePreserveChecks ? this.ast.checks : void 0), fields2);
      }
    });
  }
  __name(makeStruct, "makeStruct");
  function Struct(fields) {
    return makeStruct(struct(fields, void 0), fields);
  }
  __name(Struct, "Struct");
  function Record(key, value) {
    return make10(record(key.ast, value.ast), {
      key,
      value
    });
  }
  __name(Record, "Record");
  function makeTuple(ast, elements) {
    return make10(ast, {
      elements,
      mapElements(f, options) {
        const elements2 = f(this.elements);
        return makeTuple(tuple(elements2, options?.unsafePreserveChecks ? this.ast.checks : void 0), elements2);
      }
    });
  }
  __name(makeTuple, "makeTuple");
  function Tuple(elements) {
    return makeTuple(tuple(elements), elements);
  }
  __name(Tuple, "Tuple");
  var ArraySchema = /* @__PURE__ */ lambda((schema) => make10(new Arrays(false, [], [schema.ast]), {
    value: schema
  }));
  function makeUnion(ast, members) {
    return make10(ast, {
      members,
      mapMembers(f, options) {
        const members2 = f(this.members);
        return makeUnion(union(members2, this.ast.options, options?.unsafePreserveChecks ? this.ast.checks : void 0), members2);
      }
    });
  }
  __name(makeUnion, "makeUnion");
  function Union2(members, options) {
    return makeUnion(union(members, options, void 0), members);
  }
  __name(Union2, "Union");
  function Literals(literals) {
    const members = literals.map(Literal2);
    return make10(union(members, void 0, void 0), {
      literals,
      members,
      mapMembers(f) {
        return Union2(f(this.members));
      },
      pick(literals2) {
        return Literals(literals2);
      },
      transform(to) {
        return Union2(members.map((member, index) => member.transform(to[index])));
      }
    });
  }
  __name(Literals, "Literals");
  var NullOr = /* @__PURE__ */ lambda((self) => Union2([self, Null2]));
  function decodeTo2(to, transformation) {
    return (from) => {
      return make10(decodeTo(from.ast, to.ast, transformation ? makeTransformation(transformation) : passthrough3()), {
        from,
        to
      });
    };
  }
  __name(decodeTo2, "decodeTo");
  function withConstructorDefault2(defaultValue) {
    return (schema) => make10(withConstructorDefault(schema.ast, defaultValue), {
      schema
    });
  }
  __name(withConstructorDefault2, "withConstructorDefault");
  function tag(literal2) {
    return Literal2(literal2).pipe(withConstructorDefault2(succeed7(literal2)));
  }
  __name(tag, "tag");
  function TaggedStruct(value, fields) {
    return Struct({
      _tag: tag(value),
      ...fields
    });
  }
  __name(TaggedStruct, "TaggedStruct");
  function instanceOf(constructor, annotations) {
    return declare((u) => u instanceof constructor, annotations);
  }
  __name(instanceOf, "instanceOf");
  function link() {
    return (encodeTo, transformation) => {
      return new Link(encodeTo.ast, makeTransformation(transformation));
    };
  }
  __name(link, "link");
  var makeFilter2 = makeFilter;
  function isPattern2(regExp, annotations) {
    const source = regExp.source;
    const flags = regExp.flags;
    const canExport = /^[dg]*uy?$/.test(flags);
    const runtimeRegExp = flags === "" ? `new RegExp(${format2(source)})` : `new RegExp(${format2(source)}, ${format2(flags)})`;
    return isPattern(regExp, {
      toJsonSchema: /* @__PURE__ */ __name(() => canExport ? {
        pattern: flags.endsWith("y") ? `^(?:${source})` : source
      } : [{}, true], "toJsonSchema"),
      toCode: /* @__PURE__ */ __name(() => ({
        runtime: `Schema.isPattern(${runtimeRegExp})`
      }), "toCode"),
      ...annotations
    });
  }
  __name(isPattern2, "isPattern");
  function isBase64(annotations) {
    const regExp = /^([0-9a-zA-Z+/]{4})*(([0-9a-zA-Z+/]{2}==)|([0-9a-zA-Z+/]{3}=))?$/;
    return isPattern2(regExp, {
      expected: "a base64 encoded string",
      representation: {
        id: "effect/schema/isBase64",
        payload: null
      },
      toJsonSchema: /* @__PURE__ */ __name(() => ({
        pattern: regExp.source
      }), "toJsonSchema"),
      toCode: /* @__PURE__ */ __name(() => ({
        runtime: "Schema.isBase64()"
      }), "toCode"),
      ...annotations
    });
  }
  __name(isBase64, "isBase64");
  var Finite = /* @__PURE__ */ make10(finite);
  function makeIsGreaterThan(options) {
    const gt = isGreaterThan(options.order);
    const formatter = options.formatter ?? format2;
    return (exclusiveMinimum, annotations) => {
      return makeFilter2((input) => gt(input, exclusiveMinimum), {
        expected: `a value greater than ${formatter(exclusiveMinimum)}`,
        arbitraryConstraint: {
          order: options.order,
          minimum: exclusiveMinimum,
          exclusiveMinimum: true
        },
        ...options.annotate?.(exclusiveMinimum),
        ...annotations
      });
    };
  }
  __name(makeIsGreaterThan, "makeIsGreaterThan");
  function makeIsGreaterThanOrEqualTo(options) {
    const gte = isGreaterThanOrEqualTo(options.order);
    const formatter = options.formatter ?? format2;
    return (minimum, annotations) => {
      return makeFilter2((input) => gte(input, minimum), {
        expected: `a value greater than or equal to ${formatter(minimum)}`,
        arbitraryConstraint: {
          order: options.order,
          minimum
        },
        ...options.annotate?.(minimum),
        ...annotations
      });
    };
  }
  __name(makeIsGreaterThanOrEqualTo, "makeIsGreaterThanOrEqualTo");
  function makeIsBetween(deriveOptions) {
    const greaterThanOrEqualTo = isGreaterThanOrEqualTo(deriveOptions.order);
    const greaterThan = isGreaterThan(deriveOptions.order);
    const lessThanOrEqualTo = isLessThanOrEqualTo(deriveOptions.order);
    const lessThan = isLessThan(deriveOptions.order);
    const formatter = deriveOptions.formatter ?? format2;
    return (options, annotations) => {
      const gte = options.exclusiveMinimum ? greaterThan : greaterThanOrEqualTo;
      const lte = options.exclusiveMaximum ? lessThan : lessThanOrEqualTo;
      return makeFilter2((input) => gte(input, options.minimum) && lte(input, options.maximum), {
        expected: `a value between ${formatter(options.minimum)}${options.exclusiveMinimum ? " (excluded)" : ""} and ${formatter(options.maximum)}${options.exclusiveMaximum ? " (excluded)" : ""}`,
        arbitraryConstraint: {
          order: deriveOptions.order,
          minimum: options.minimum,
          maximum: options.maximum,
          ...options.exclusiveMinimum && {
            exclusiveMinimum: true
          },
          ...options.exclusiveMaximum && {
            exclusiveMaximum: true
          }
        },
        ...deriveOptions.annotate?.(options),
        ...annotations
      });
    };
  }
  __name(makeIsBetween, "makeIsBetween");
  function encodeNumberPayload(number4) {
    if (!globalThis.Number.isFinite(number4)) {
      throw new globalThis.RangeError(`Expected a finite number, got ${format2(number4)}`);
    }
    return number4;
  }
  __name(encodeNumberPayload, "encodeNumberPayload");
  var isGreaterThan2 = /* @__PURE__ */ makeIsGreaterThan({
    order: Number2,
    annotate: /* @__PURE__ */ __name((exclusiveMinimum) => ({
      representation: {
        id: "effect/schema/isGreaterThan",
        payload: {
          exclusiveMinimum: encodeNumberPayload(exclusiveMinimum)
        }
      },
      toJsonSchema: /* @__PURE__ */ __name(() => ({
        exclusiveMinimum
      }), "toJsonSchema"),
      toCode: /* @__PURE__ */ __name(() => ({
        runtime: `Schema.isGreaterThan(${format2(exclusiveMinimum)})`
      }), "toCode")
    }), "annotate")
  });
  var isGreaterThanOrEqualTo2 = /* @__PURE__ */ makeIsGreaterThanOrEqualTo({
    order: Number2,
    annotate: /* @__PURE__ */ __name((minimum) => ({
      representation: {
        id: "effect/schema/isGreaterThanOrEqualTo",
        payload: {
          minimum: encodeNumberPayload(minimum)
        }
      },
      toJsonSchema: /* @__PURE__ */ __name(() => ({
        minimum
      }), "toJsonSchema"),
      toCode: /* @__PURE__ */ __name(() => ({
        runtime: `Schema.isGreaterThanOrEqualTo(${format2(minimum)})`
      }), "toCode")
    }), "annotate")
  });
  var isBetween2 = /* @__PURE__ */ makeIsBetween({
    order: Number2,
    annotate: /* @__PURE__ */ __name((options) => {
      const exclusiveMinimum = options.exclusiveMinimum ? true : void 0;
      const exclusiveMaximum = options.exclusiveMaximum ? true : void 0;
      const payload = {
        minimum: encodeNumberPayload(options.minimum),
        maximum: encodeNumberPayload(options.maximum),
        ...exclusiveMinimum && {
          exclusiveMinimum
        },
        ...exclusiveMaximum && {
          exclusiveMaximum
        }
      };
      return {
        representation: {
          id: "effect/schema/isBetween",
          payload
        },
        toJsonSchema: /* @__PURE__ */ __name(() => ({
          [exclusiveMinimum ? "exclusiveMinimum" : "minimum"]: options.minimum,
          [exclusiveMaximum ? "exclusiveMaximum" : "maximum"]: options.maximum
        }), "toJsonSchema"),
        toCode: /* @__PURE__ */ __name(() => ({
          runtime: `Schema.isBetween({ minimum: ${format2(options.minimum)}, maximum: ${format2(options.maximum)}, exclusiveMinimum: ${format2(exclusiveMinimum)}, exclusiveMaximum: ${format2(exclusiveMaximum)} })`
        }), "toCode")
      };
    }, "annotate")
  });
  function isInt(annotations) {
    return makeFilter2((n) => globalThis.Number.isSafeInteger(n), {
      expected: "an integer",
      representation: {
        id: "effect/schema/isInt",
        payload: null
      },
      toJsonSchema: /* @__PURE__ */ __name(() => ({
        type: "integer",
        minimum: globalThis.Number.MIN_SAFE_INTEGER,
        maximum: globalThis.Number.MAX_SAFE_INTEGER
      }), "toJsonSchema"),
      toCode: /* @__PURE__ */ __name(() => ({
        runtime: "Schema.isInt()"
      }), "toCode"),
      arbitraryConstraint: {
        number: "integer"
      },
      ...annotations
    });
  }
  __name(isInt, "isInt");
  var Int = /* @__PURE__ */ Number6.check(/* @__PURE__ */ isInt());
  function makeIsMinLength(minLength, minCodePoints, annotations) {
    return makeFilter2((input) => input.length >= minLength, {
      expected: `a value with a length of at least ${minLength}`,
      representation: {
        id: "effect/schema/isMinLength",
        payload: {
          minLength
        }
      },
      toJsonSchema: /* @__PURE__ */ __name(({
        type
      }) => type === "string" ? minLength <= 1 ? {
        minLength: minCodePoints
      } : [{
        minLength: minCodePoints
      }, true] : type === "array" ? {
        minItems: minLength
      } : type === void 0 ? [{
        minLength: minCodePoints,
        minItems: minLength
      }, true] : [{}, true], "toJsonSchema"),
      toCode: /* @__PURE__ */ __name(() => ({
        runtime: `Schema.isMinLength(${minLength})`
      }), "toCode"),
      [STRUCTURAL_ANNOTATION_KEY]: true,
      arbitraryConstraint: {
        minLength
      },
      ...annotations
    });
  }
  __name(makeIsMinLength, "makeIsMinLength");
  function isNonEmpty(annotations) {
    return makeIsMinLength(1, 1, annotations);
  }
  __name(isNonEmpty, "isNonEmpty");
  var getErrorOptionsKey = /* @__PURE__ */ __name((options) => (options?.includeStack === true ? 1 : 0) | (options?.excludeCause === true ? 2 : 0), "getErrorOptionsKey");
  var getErrorOptions = /* @__PURE__ */ __name((key) => {
    switch (key) {
      case 0:
        return void 0;
      case 1:
        return {
          includeStack: true
        };
      case 2:
        return {
          excludeCause: true
        };
      case 3:
        return {
          includeStack: true,
          excludeCause: true
        };
    }
  }, "getErrorOptions");
  var defectSchemaCache = [];
  function Defect(options) {
    const key = getErrorOptionsKey(options);
    const cached2 = defectSchemaCache[key];
    if (cached2 !== void 0) {
      return cached2;
    }
    const schema = Json2.pipe(decodeTo2(Unknown2, defectFromJson(getErrorOptions(key))));
    defectSchemaCache[key] = schema;
    return schema;
  }
  __name(Defect, "Defect");
  var RegExp2 = /* @__PURE__ */ instanceOf(globalThis.RegExp, {
    representation: {
      id: "effect/schema/RegExp",
      payload: null
    },
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: `Schema.RegExp`,
      Type: `globalThis.RegExp`
    }), "toCode"),
    expected: "RegExp",
    toCodecJson: /* @__PURE__ */ __name(() => link()(Struct({
      source: String4,
      flags: String4
    }), transformEffect2({
      decode: /* @__PURE__ */ __name((e, options) => try_2({
        try: /* @__PURE__ */ __name(() => new globalThis.RegExp(e.source, e.flags), "try"),
        catch: /* @__PURE__ */ __name(() => new InvalidValue({
          expected: "valid RegExp source and flags"
        }, e, options), "catch")
      }), "decode"),
      encode: /* @__PURE__ */ __name((regExp) => succeed7({
        source: regExp.source,
        flags: regExp.flags
      }), "encode")
    })), "toCodecJson")
  });
  var URLString = /* @__PURE__ */ String4.annotate({
    expected: "a string that will be decoded as a URL"
  });
  var URL3 = /* @__PURE__ */ instanceOf(globalThis.URL, {
    representation: {
      id: "effect/schema/URL",
      payload: null
    },
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: `Schema.URL`,
      Type: `globalThis.URL`
    }), "toCode"),
    expected: "URL",
    toCodecJson: /* @__PURE__ */ __name(() => link()(URLString, urlFromString), "toCodecJson")
  });
  var DateString = /* @__PURE__ */ String4.annotate({
    expected: "a string that will be decoded as a Date"
  });
  var Date4 = /* @__PURE__ */ declare((input) => input instanceof globalThis.Date && !globalThis.Number.isNaN(input.getTime()), {
    representation: {
      id: "effect/schema/Date",
      payload: null
    },
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: `Schema.Date`,
      Type: `globalThis.Date`
    }), "toCode"),
    expected: "a valid Date",
    toCodecJson: /* @__PURE__ */ __name(() => link()(DateString, dateFromString), "toCodecJson")
  });
  var File = /* @__PURE__ */ instanceOf(globalThis.File, {
    representation: {
      id: "effect/schema/File",
      payload: null
    },
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: `Schema.File`,
      Type: `globalThis.File`
    }), "toCode"),
    expected: "File",
    toCodecJson: /* @__PURE__ */ __name(() => link()(Struct({
      data: String4.check(isBase64()),
      type: String4,
      name: String4,
      lastModified: Int
    }), transformEffect2({
      decode: /* @__PURE__ */ __name((e, options) => match3(decode(e.data), {
        onFailure: /* @__PURE__ */ __name(() => fail5(new InvalidValue({
          expected: "a valid Base64 string"
        }, e.data, options)), "onFailure"),
        onSuccess: /* @__PURE__ */ __name((bytes) => {
          const buffer2 = new globalThis.Uint8Array(bytes);
          return succeed7(new globalThis.File([buffer2], e.name, {
            type: e.type,
            lastModified: e.lastModified
          }));
        }, "onSuccess")
      }), "decode"),
      encode: /* @__PURE__ */ __name((file, options) => tryPromise2({
        try: /* @__PURE__ */ __name(async () => {
          const bytes = new globalThis.Uint8Array(await file.arrayBuffer());
          return {
            data: encode(bytes),
            type: file.type,
            name: file.name,
            lastModified: file.lastModified
          };
        }, "try"),
        catch: /* @__PURE__ */ __name(() => new InvalidValue({
          expected: "a readable File"
        }, file, options), "catch")
      }), "encode")
    })), "toCodecJson")
  });
  var FormData2 = /* @__PURE__ */ instanceOf(globalThis.FormData, {
    representation: {
      id: "effect/schema/FormData",
      payload: null
    },
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: `Schema.FormData`,
      Type: `globalThis.FormData`
    }), "toCode"),
    expected: "FormData",
    toCodecJson: /* @__PURE__ */ __name(() => link()(ArraySchema(Tuple([String4, Union2([Struct({
      _tag: tag("String"),
      value: String4
    }), Struct({
      _tag: tag("File"),
      value: File
    })])])), transformEffect2({
      decode: /* @__PURE__ */ __name((e) => {
        const out = new globalThis.FormData();
        for (const [key, entry] of e) {
          out.append(key, entry.value);
        }
        return succeed7(out);
      }, "decode"),
      encode: /* @__PURE__ */ __name((formData) => {
        return succeed7(globalThis.Array.from(formData.entries()).map(([key, value]) => {
          if (typeof value === "string") {
            return [key, {
              _tag: "String",
              value
            }];
          } else {
            return [key, {
              _tag: "File",
              value
            }];
          }
        }));
      }, "encode")
    })), "toCodecJson")
  });
  var URLSearchParams3 = /* @__PURE__ */ instanceOf(globalThis.URLSearchParams, {
    representation: {
      id: "effect/schema/URLSearchParams",
      payload: null
    },
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: `Schema.URLSearchParams`,
      Type: `globalThis.URLSearchParams`
    }), "toCode"),
    expected: "URLSearchParams",
    toCodecJson: /* @__PURE__ */ __name(() => link()(String4.annotate({
      expected: "a query string that will be decoded as URLSearchParams"
    }), transform2({
      decode: /* @__PURE__ */ __name((e) => new globalThis.URLSearchParams(e), "decode"),
      encode: /* @__PURE__ */ __name((params) => params.toString(), "encode")
    })), "toCodecJson")
  });
  var Base64String = /* @__PURE__ */ String4.annotate({
    expected: "a base64 encoded string that will be decoded as Uint8Array",
    format: "byte",
    contentEncoding: "base64"
  });
  var Uint8Array2 = /* @__PURE__ */ instanceOf(globalThis.Uint8Array, {
    representation: {
      id: "effect/schema/Uint8Array",
      payload: null
    },
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: `Schema.Uint8Array`,
      Type: `globalThis.Uint8Array`
    }), "toCode"),
    expected: "Uint8Array",
    toCodecJson: /* @__PURE__ */ __name(() => link()(Base64String, uint8ArrayFromBase64String), "toCodecJson")
  });
  var arbitraryMinimumDateTimestamp = -864e13;
  var arbitraryMaximumDateTimestamp = 864e13;
  var arbitraryMinimumZonedDateTimeTimestamp = arbitraryMinimumDateTimestamp + 14 * 60 * 60 * 1e3;
  var arbitraryMaximumZonedDateTimeTimestamp = arbitraryMaximumDateTimestamp - 14 * 60 * 60 * 1e3;
  var arbitraryMinimumTimeZoneOffset = -12 * 60 * 60 * 1e3;
  var arbitraryMaximumTimeZoneOffset = 14 * 60 * 60 * 1e3;
  var immerable = /* @__PURE__ */ globalThis.Symbol.for("immer-draftable");
  var payloadToken = {};
  function makeClass(Inherited, identifier2, struct2, annotations, proto) {
    const getClassSchema = getClassSchemaFactory(struct2, identifier2, annotations);
    const ClassTypeId = getClassTypeId(identifier2);
    const out = class extends Inherited {
      static {
        __name(this, "out");
      }
      constructor(...[input, options]) {
        const internalOptions = options;
        const payload = internalOptions?.["~payload"];
        const value = payload?.token === payloadToken ? payload.value : struct2.make(input ?? {}, options);
        super(value, {
          ...options,
          disableChecks: true,
          "~payload": {
            token: payloadToken,
            value
          }
        });
      }
      static [TypeId13] = TypeId13;
      get [ClassTypeId]() {
        return ClassTypeId;
      }
      static [immerable] = true;
      static identifier = identifier2;
      static fields = struct2.fields;
      static get ast() {
        return getClassSchema(this).ast;
      }
      static pipe() {
        return pipeArguments(this, arguments);
      }
      static rebuild(ast) {
        return getClassSchema(this).rebuild(ast);
      }
      static make(input, options) {
        return make8(getClassSchema(this))(input ?? {}, options);
      }
      static makeOption(input, options) {
        return makeOption(getClassSchema(this))(input ?? {}, options);
      }
      static makeEffect(input, options) {
        return getClassSchema(this).makeEffect(input ?? {}, options);
      }
      static annotate(annotations2) {
        return this.rebuild(annotate(this.ast, annotations2));
      }
      static annotateKey(annotations2) {
        return this.rebuild(annotateKey(this.ast, annotations2));
      }
      static check(...checks) {
        return this.rebuild(appendChecks(this.ast, checks));
      }
      static extend(identifier3) {
        return (schema, annotations2) => {
          const extension = isStruct(schema) ? schema : Struct(schema);
          const fields = {
            ...struct2.fields,
            ...extension.fields
          };
          const ast = struct(fields, struct2.ast.checks, {
            identifier: identifier3
          });
          return makeClass(this, identifier3, makeStruct(appendChecks(ast, extension.ast.checks), fields), annotations2, proto);
        };
      }
      static mapFields(f, options) {
        return struct2.mapFields(f, options);
      }
    };
    if (proto !== void 0) {
      Object.assign(out.prototype, proto(identifier2));
    }
    return out;
  }
  __name(makeClass, "makeClass");
  function getClassTransformation(self) {
    return new Transformation(transform((input) => new self(input, {
      "~payload": {
        token: payloadToken,
        value: input
      }
    })), passthrough2());
  }
  __name(getClassTransformation, "getClassTransformation");
  function getClassTypeId(identifier2) {
    return `~effect/Schema/Class/${identifier2}`;
  }
  __name(getClassTypeId, "getClassTypeId");
  function getClassSchemaFactory(from, identifier2, annotations) {
    let memo;
    return (self) => {
      if (memo !== void 0) {
        return memo;
      }
      const ClassTypeId = getClassTypeId(identifier2);
      const isClassValue = /* @__PURE__ */ __name((input) => input instanceof self || hasProperty(input, ClassTypeId), "isClassValue");
      const transformation = getClassTransformation(self);
      const to = make10(new Declaration([from.ast], () => (input, ast, options) => {
        return isClassValue(input) ? succeed7(input) : fail5(new InvalidType(ast, input, options));
      }, {
        identifier: identifier2,
        [CONSTRUCTOR_ANNOTATION_KEY]: ([from2]) => ({
          isConstructed: isClassValue,
          link: new Link(from2, transformation)
        }),
        toCodec: /* @__PURE__ */ __name(([from2]) => new Link(from2.ast, transformation), "toCodec"),
        toEquivalence: /* @__PURE__ */ __name(([from2]) => from2, "toEquivalence"),
        toFormatter: /* @__PURE__ */ __name(([from2]) => (t) => `${self.identifier}(${from2(t)})`, "toFormatter"),
        [SENTINELS_ANNOTATION_KEY]: collectSentinels(from.ast),
        ...annotations
      }));
      return memo = decodeTo2(to, transformation)(from);
    };
  }
  __name(getClassSchemaFactory, "getClassSchemaFactory");
  function isStruct(schema) {
    return isSchema(schema);
  }
  __name(isStruct, "isStruct");
  var Error3 = /* @__PURE__ */ __name((identifier2) => (schema, annotations) => {
    const struct2 = isStruct(schema) ? schema : Struct(schema);
    const self = makeClass(Error2, identifier2, struct2, annotations, (identifier3) => ({
      name: identifier3
    }));
    return self;
  }, "Error");
  var TaggedError3 = /* @__PURE__ */ __name((identifier2) => {
    return (tagValue, schema, annotations) => {
      const struct2 = isStruct(schema) ? schema.mapFields((fields) => ({
        _tag: tag(tagValue),
        ...fields
      }), {
        unsafePreserveChecks: true
      }) : TaggedStruct(tagValue, schema);
      return Error3(identifier2 ?? tagValue)(struct2, annotations);
    };
  }, "TaggedError");
  var Json2 = /* @__PURE__ */ make10(/* @__PURE__ */ annotate(Json, {
    toCode: /* @__PURE__ */ __name(() => ({
      runtime: "Schema.Json",
      Type: "Schema.Json"
    }), "toCode")
  }));

  // packages/runtime/src/game/flash/contract/Diagnostic.ts
  var diagnosticTimestamp = /* @__PURE__ */ __name(() => performance.timeOrigin + performance.now(), "diagnosticTimestamp");
  var makeDiagnostic = /* @__PURE__ */ __name((phase, operation, cause, args2) => ({
    ...args2 === void 0 ? {} : { arguments: args2 },
    cause,
    operation,
    phase,
    timestamp: diagnosticTimestamp()
  }), "makeDiagnostic");

  // packages/runtime/src/game/flash/bridge/DiagnosticSink.ts
  var noop = {
    report: /* @__PURE__ */ __name(() => void 0, "report")
  };
  var debug = {
    report: /* @__PURE__ */ __name((phase, operation, cause, args2) => {
      console.warn(
        "[flash:diagnostic]",
        makeDiagnostic(phase, operation, cause, args2)
      );
    }, "report")
  };
  var DiagnosticSink = class extends Reference(
    "lucent/renderer/flash/DiagnosticSink",
    { defaultValue: /* @__PURE__ */ __name(() => noop, "defaultValue") }
  ) {
    static {
      __name(this, "DiagnosticSink");
    }
  };
  var noopLayer = succeed6(DiagnosticSink, noop);
  var debugLayer = succeed6(DiagnosticSink, debug);

  // packages/runtime/src/game/flash/bridge/Bridge.ts
  var InvokeFailure = class extends TaggedError2("InvokeFailure") {
    static {
      __name(this, "InvokeFailure");
    }
  };
  var makeBridge = /* @__PURE__ */ __name((target) => gen2(function* () {
    const swf = (target ?? window).swf;
    const diagnosticSink = yield* DiagnosticSink;
    const fail7 = /* @__PURE__ */ __name((method, cause, args2) => sync2(
      () => diagnosticSink.report(
        "invoke",
        String(method),
        cause,
        method === "auth.login" ? void 0 : args2
      )
    ).pipe(as2(none2())), "fail");
    const invoke = /* @__PURE__ */ __name((method, args2, schema) => try_2({
      try: /* @__PURE__ */ __name(() => {
        const candidate = swf[method];
        if (typeof candidate !== "function") {
          throw new Error(`Missing Flash method: ${String(method)}`);
        }
        return Reflect.apply(candidate, swf, [...args2 ?? []]);
      }, "try"),
      catch: /* @__PURE__ */ __name((cause) => new InvokeFailure({ cause }), "catch")
    }).pipe(
      flatMap3(decodeUnknownEffect2(schema)),
      map5(some2),
      catch_2((cause) => fail7(method, cause, args2))
    ), "invoke");
    const invokeJson = /* @__PURE__ */ __name((method, args2, schema) => invoke(method, args2, Union2([String4, Unknown2])).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => succeed7(none2()), "onNone"),
          onSome: /* @__PURE__ */ __name((value) => {
            if (typeof value !== "string") {
              return decodeUnknownEffect2(schema)(value).pipe(
                map5(some2),
                catch_2((cause) => fail7(method, cause, args2))
              );
            }
            return try_2({
              try: /* @__PURE__ */ __name(() => JSON.parse(value), "try"),
              catch: /* @__PURE__ */ __name((cause) => new InvokeFailure({ cause }), "catch")
            }).pipe(
              flatMap3(decodeUnknownEffect2(schema)),
              map5(some2),
              catch_2((cause) => fail7(method, cause, args2))
            );
          }, "onSome")
        })
      )
    ), "invokeJson");
    return {
      invoke,
      invokeJson
    };
  }), "makeBridge");
  var Bridge = class extends Service()(
    "lucent/renderer/flash/Bridge",
    { make: makeBridge() }
  ) {
    static {
      __name(this, "Bridge");
    }
  };
  var layer = effect(Bridge, Bridge.make);

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Latch.js
  var makeUnsafe4 = makeLatchUnsafe;

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/MutableList.js
  var Empty = /* @__PURE__ */ Symbol.for("effect/MutableList/Empty");
  var make11 = /* @__PURE__ */ __name(() => ({
    head: void 0,
    tail: void 0,
    length: 0
  }), "make");
  var compactHead = /* @__PURE__ */ __name((self) => {
    const head2 = self.head;
    if (head2.offset >= 1024 && head2 === self.tail && head2.mutable && (head2.array.length - head2.offset) * 8 <= head2.offset) {
      self.head = self.tail = {
        array: head2.array.slice(head2.offset),
        mutable: true,
        offset: 0,
        next: void 0
      };
    }
  }, "compactHead");
  var emptyBucket = /* @__PURE__ */ __name(() => ({
    array: [],
    mutable: true,
    offset: 0,
    next: void 0
  }), "emptyBucket");
  var append2 = /* @__PURE__ */ __name((self, message2) => {
    if (!self.tail) {
      self.head = self.tail = emptyBucket();
    } else if (!self.tail.mutable) {
      self.tail.next = emptyBucket();
      self.tail = self.tail.next;
    }
    self.tail.array.push(message2);
    self.length++;
  }, "append");
  var prepend2 = /* @__PURE__ */ __name((self, message2) => {
    self.head = {
      array: [message2],
      mutable: true,
      offset: 0,
      next: self.head
    };
    if (!self.tail) self.tail = self.head;
    self.length++;
  }, "prepend");
  var clear2 = /* @__PURE__ */ __name((self) => {
    self.head = self.tail = void 0;
    self.length = 0;
  }, "clear");
  var takeN = /* @__PURE__ */ __name((self, n) => {
    n = normalize(n);
    if (n <= 0 || !self.head) return [];
    n = Math.min(n, self.length);
    if (n === self.length && self.head?.offset === 0 && !self.head.next) {
      const array3 = self.head.array;
      clear2(self);
      return array3;
    }
    const array2 = new Array(n);
    let index = 0;
    let chunk = self.head;
    while (chunk) {
      while (chunk.offset < chunk.array.length) {
        array2[index++] = chunk.array[chunk.offset];
        if (chunk.mutable) chunk.array[chunk.offset] = void 0;
        chunk.offset++;
        if (index === n) {
          self.head = chunk.offset === chunk.array.length && chunk.next ? chunk.next : chunk;
          self.length -= n;
          if (self.length === 0) clear2(self);
          else compactHead(self);
          return array2;
        }
      }
      chunk = chunk.next;
    }
    clear2(self);
    return array2;
  }, "takeN");
  var takeAll = /* @__PURE__ */ __name((self) => takeN(self, self.length), "takeAll");
  var take = /* @__PURE__ */ __name((self) => {
    if (!self.head) return Empty;
    const message2 = self.head.array[self.head.offset];
    if (self.head.mutable) self.head.array[self.head.offset] = void 0;
    self.head.offset++;
    self.length--;
    if (self.head.offset === self.head.array.length) {
      if (self.head.next) {
        self.head = self.head.next;
      } else {
        clear2(self);
      }
    } else {
      compactHead(self);
    }
    return message2;
  }, "take");
  var filter3 = /* @__PURE__ */ __name((self, f) => {
    const array2 = [];
    let chunk = self.head;
    let index = 0;
    while (chunk) {
      for (let i = chunk.offset; i < chunk.array.length; i++) {
        if (f(chunk.array[i], index++)) {
          array2.push(chunk.array[i]);
        }
      }
      chunk = chunk.next;
    }
    if (array2.length === 0) {
      clear2(self);
      return;
    }
    self.head = self.tail = {
      array: array2,
      mutable: true,
      offset: 0,
      next: void 0
    };
    self.length = array2.length;
  }, "filter");
  var remove = /* @__PURE__ */ __name((self, value) => filter3(self, (v) => v !== value), "remove");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/MutableRef.js
  var TypeId14 = "~effect/MutableRef";
  var MutableRefProto = {
    [TypeId14]: TypeId14,
    ...PipeInspectableProto,
    toJSON() {
      return {
        _id: "MutableRef",
        current: toJson(this.current)
      };
    }
  };
  var make12 = /* @__PURE__ */ __name((value) => {
    const ref = Object.create(MutableRefProto);
    ref.current = value;
    return ref;
  }, "make");
  var set = /* @__PURE__ */ dual(2, (self, value) => {
    self.current = value;
    return self;
  });

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/PubSub.js
  var TypeId15 = "~effect/PubSub";
  var SubscriptionTypeId = "~effect/PubSub/Subscription";
  var make13 = /* @__PURE__ */ __name((options) => sync2(() => makePubSubUnsafe(options.atomicPubSub(), /* @__PURE__ */ new Map(), makeUnsafe3(), makeUnsafe4(false), make12(false), options.strategy(), make12(none2()))), "make");
  var unbounded = /* @__PURE__ */ __name((options) => make13({
    atomicPubSub: /* @__PURE__ */ __name(() => makeAtomicUnbounded(options), "atomicPubSub"),
    strategy: /* @__PURE__ */ __name(() => new DroppingStrategy(), "strategy")
  }), "unbounded");
  var makeAtomicUnbounded = /* @__PURE__ */ __name((options) => {
    const replay = options?.replay;
    return new UnboundedPubSub(replay && replay > 0 ? new ReplayBuffer(Math.ceil(replay)) : void 0);
  }, "makeAtomicUnbounded");
  var shutdown = /* @__PURE__ */ __name((self) => uninterruptible2(withFiber2((fiber2) => {
    set(self.shutdownFlag, true);
    return close(self.scope, interrupt2(fiber2.id)).pipe(andThen2(self.strategy.shutdown), when2(self.shutdownHook.open), asVoid2);
  })), "shutdown");
  var publish = /* @__PURE__ */ dual(2, (self, value) => suspend2(() => {
    if (self.shutdownFlag.current || isSome2(self.ended.current)) {
      return succeed7(false);
    }
    if (self.pubsub.publish(value)) {
      self.strategy.completeSubscribersUnsafe(self.pubsub, self.subscribers);
      return succeed7(true);
    }
    return self.strategy.handleSurplus(self.pubsub, self.subscribers, [value], self.shutdownFlag, self.ended);
  }));
  var publishUnsafe = /* @__PURE__ */ dual(2, (self, value) => {
    if (self.shutdownFlag.current || isSome2(self.ended.current)) return false;
    if (self.pubsub.publish(value)) {
      self.strategy.completeSubscribersUnsafe(self.pubsub, self.subscribers);
      return true;
    }
    return false;
  });
  var subscribe = /* @__PURE__ */ __name((self) => uninterruptible2(contextWith2((services) => {
    const localScope = get(services, Scope);
    const scope3 = forkUnsafe2(self.scope);
    const subscription = makeSubscriptionUnsafe(self.pubsub, self.subscribers, self.strategy, self.ended);
    return addFinalizer2(scope3, unsubscribe(subscription)).pipe(andThen2(addFinalizerExit(localScope, (exit3) => close(scope3, exit3))), as2(subscription));
  })), "subscribe");
  var unsubscribe = /* @__PURE__ */ __name((self) => uninterruptible2(withFiber2((state) => {
    set(self.shutdownFlag, true);
    return forEach2(takeAll(self.pollers), (d) => interruptWith(d, state.id), {
      discard: true,
      concurrency: "unbounded"
    }).pipe(tap2(() => sync2(() => {
      self.subscribers.delete(self.subscription);
      self.subscription.unsubscribe();
      self.replayWindow.close();
      self.strategy.onPubSubEmptySpaceUnsafe(self.pubsub, self.subscribers);
    })), when2(self.shutdownHook.open), asVoid2);
  })), "unsubscribe");
  var take2 = /* @__PURE__ */ __name((self) => suspend2(() => {
    if (self.shutdownFlag.current) {
      return interrupt3;
    }
    if (self.replayWindow.remaining > 0) {
      const message3 = self.replayWindow.take();
      return succeed7(message3);
    }
    const message2 = self.pollers.length === 0 ? self.subscription.poll() : Empty;
    if (message2 === Empty) {
      return pollForItem(self);
    } else {
      self.strategy.onPubSubEmptySpaceUnsafe(self.pubsub, self.subscribers);
      return succeed7(message2);
    }
  }), "take");
  var takeAll2 = /* @__PURE__ */ __name((self) => suspend2(/* @__PURE__ */ __name(function loop(value) {
    if (self.shutdownFlag.current) {
      return interrupt3;
    }
    let as3 = self.pollers.length === 0 ? self.subscription.pollUpTo(Number.POSITIVE_INFINITY) : [];
    if (value) {
      as3 = value.concat(as3);
    }
    self.strategy.onPubSubEmptySpaceUnsafe(self.pubsub, self.subscribers);
    if (self.replayWindow.remaining > 0) {
      return succeed7(self.replayWindow.takeAll().concat(as3));
    } else if (!isArrayNonEmpty2(as3)) {
      return flatMap3(pollForItem(self), (item) => loop([item]));
    }
    return succeed7(as3);
  }, "loop")), "takeAll");
  var pollForItem = /* @__PURE__ */ __name((self) => callback2((resume) => {
    if (self.shutdownFlag.current) return resume(interrupt3);
    if (isSome2(self.ended.current)) {
      const message2 = self.pollers.length === 0 ? self.subscription.poll() : Empty;
      if (message2 !== Empty) {
        self.strategy.onPubSubEmptySpaceUnsafe(self.pubsub, self.subscribers);
        return resume(succeed7(message2));
      }
      return resume(succeed7(self.ended.current.value));
    }
    const deferred = makeUnsafe2();
    let set4 = self.subscribers.get(self.subscription);
    if (!set4) {
      set4 = /* @__PURE__ */ new Set();
      self.subscribers.set(self.subscription, set4);
    }
    set4.add(self.pollers);
    append2(self.pollers, deferred);
    self.strategy.completePollersUnsafe(self.pubsub, self.subscribers, self.subscription, self.pollers);
    if (deferred.effect) return resume(deferred.effect);
    deferred.resumes = [resume];
    return sync2(() => remove(self.pollers, deferred));
  }), "pollForItem");
  var AbsentValue = /* @__PURE__ */ Symbol.for("effect/PubSub/AbsentValue");
  var addSubscribers = /* @__PURE__ */ __name((subscribers, subscription, pollers) => {
    if (!subscribers.has(subscription)) {
      subscribers.set(subscription, /* @__PURE__ */ new Set());
    }
    const set4 = subscribers.get(subscription);
    set4.add(pollers);
  }, "addSubscribers");
  var removeSubscribers = /* @__PURE__ */ __name((subscribers, subscription, pollers) => {
    if (!subscribers.has(subscription)) {
      return;
    }
    const set4 = subscribers.get(subscription);
    set4.delete(pollers);
    if (set4.size === 0) {
      subscribers.delete(subscription);
    }
  }, "removeSubscribers");
  var makeSubscriptionUnsafe = /* @__PURE__ */ __name((pubsub, subscribers, strategy, ended) => new SubscriptionImpl(pubsub, subscribers, pubsub.subscribe(), make11(), makeUnsafe4(false), make12(false), strategy, pubsub.replayWindow(), ended), "makeSubscriptionUnsafe");
  var UnboundedPubSub = class {
    static {
      __name(this, "UnboundedPubSub");
    }
    publisherHead = {
      value: AbsentValue,
      replayIndex: void 0,
      subscribers: 0,
      next: null
    };
    publisherTail = this.publisherHead;
    publisherIndex = 0;
    subscribersIndex = 0;
    capacity = Number.MAX_SAFE_INTEGER;
    replayBuffer;
    constructor(replayBuffer) {
      this.replayBuffer = replayBuffer;
    }
    replayWindow() {
      return this.replayBuffer ? new ReplayWindowImpl(this.replayBuffer) : emptyReplayWindow;
    }
    isEmpty() {
      return this.publisherHead === this.publisherTail;
    }
    isFull() {
      return false;
    }
    size() {
      return this.publisherIndex - this.subscribersIndex;
    }
    publish(value) {
      const replayIndex = this.replayBuffer?.offer(value);
      const subscribers = this.publisherTail.subscribers;
      if (subscribers !== 0) {
        const node = {
          value,
          replayIndex,
          subscribers,
          next: null
        };
        this.publisherTail.next = node;
        this.publisherTail = this.publisherTail.next;
        this.publisherIndex += 1;
      }
      return true;
    }
    publishAll(elements) {
      if (this.publisherTail.subscribers !== 0) {
        for (const a of elements) {
          this.publish(a);
        }
      } else if (this.replayBuffer) {
        this.replayBuffer.offerAll(elements);
      }
      return [];
    }
    slide() {
      if (this.publisherHead !== this.publisherTail) {
        const node = this.publisherHead.next;
        const value = node.value;
        this.publisherHead = this.publisherHead.next;
        this.publisherHead.value = AbsentValue;
        this.subscribersIndex += 1;
        this.replayBuffer?.slide(value, node.replayIndex);
      }
    }
    subscribe() {
      this.publisherTail.subscribers += 1;
      return new UnboundedPubSubSubscription(this, this.publisherTail, this.publisherIndex, false);
    }
  };
  var UnboundedPubSubSubscription = class {
    static {
      __name(this, "UnboundedPubSubSubscription");
    }
    self;
    subscriberHead;
    subscriberIndex;
    unsubscribed;
    constructor(self, subscriberHead, subscriberIndex, unsubscribed) {
      this.self = self;
      this.subscriberHead = subscriberHead;
      this.subscriberIndex = subscriberIndex;
      this.unsubscribed = unsubscribed;
    }
    isEmpty() {
      if (this.unsubscribed) {
        return true;
      }
      let empty4 = true;
      let loop = true;
      while (loop) {
        if (this.subscriberHead === this.self.publisherTail) {
          loop = false;
        } else {
          if (this.subscriberHead.next.value !== AbsentValue) {
            empty4 = false;
            loop = false;
          } else {
            this.subscriberHead = this.subscriberHead.next;
            this.subscriberIndex += 1;
          }
        }
      }
      return empty4;
    }
    size() {
      if (this.unsubscribed) {
        return 0;
      }
      return this.self.publisherIndex - Math.max(this.subscriberIndex, this.self.subscribersIndex);
    }
    poll() {
      if (this.unsubscribed) {
        return Empty;
      }
      let loop = true;
      let polled = Empty;
      while (loop) {
        if (this.subscriberHead === this.self.publisherTail) {
          loop = false;
        } else {
          const elem = this.subscriberHead.next.value;
          if (elem !== AbsentValue) {
            polled = elem;
            this.subscriberHead.subscribers -= 1;
            if (this.subscriberHead.subscribers === 0) {
              this.self.publisherHead = this.self.publisherHead.next;
              this.self.publisherHead.value = AbsentValue;
              this.self.subscribersIndex += 1;
            }
            loop = false;
          }
          this.subscriberHead = this.subscriberHead.next;
          this.subscriberIndex += 1;
        }
      }
      return polled;
    }
    pollUpTo(n) {
      n = normalize(n);
      const builder = [];
      let i = 0;
      while (i !== n) {
        const a = this.poll();
        if (a === Empty) {
          i = n;
        } else {
          builder.push(a);
          i += 1;
        }
      }
      return builder;
    }
    unsubscribe() {
      if (!this.unsubscribed) {
        this.unsubscribed = true;
        this.self.publisherTail.subscribers -= 1;
        while (this.subscriberHead !== this.self.publisherTail) {
          if (this.subscriberHead.next.value !== AbsentValue) {
            this.subscriberHead.subscribers -= 1;
            if (this.subscriberHead.subscribers === 0) {
              this.self.publisherHead = this.self.publisherHead.next;
              this.self.publisherHead.value = AbsentValue;
              this.self.subscribersIndex += 1;
            }
          }
          this.subscriberHead = this.subscriberHead.next;
        }
      }
    }
  };
  var SubscriptionImpl = class {
    static {
      __name(this, "SubscriptionImpl");
    }
    [SubscriptionTypeId] = {
      _A: identity
    };
    pubsub;
    subscribers;
    subscription;
    pollers;
    shutdownHook;
    shutdownFlag;
    strategy;
    replayWindow;
    ended;
    constructor(pubsub, subscribers, subscription, pollers, shutdownHook, shutdownFlag, strategy, replayWindow, ended) {
      this.pubsub = pubsub;
      this.subscribers = subscribers;
      this.subscription = subscription;
      this.pollers = pollers;
      this.shutdownHook = shutdownHook;
      this.shutdownFlag = shutdownFlag;
      this.strategy = strategy;
      this.replayWindow = replayWindow;
      this.ended = ended;
    }
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var PubSubImpl = class {
    static {
      __name(this, "PubSubImpl");
    }
    [TypeId15] = {
      _A: identity
    };
    pubsub;
    subscribers;
    scope;
    shutdownHook;
    shutdownFlag;
    strategy;
    ended;
    constructor(pubsub, subscribers, scope3, shutdownHook, shutdownFlag, strategy, ended) {
      this.pubsub = pubsub;
      this.subscribers = subscribers;
      this.scope = scope3;
      this.shutdownHook = shutdownHook;
      this.shutdownFlag = shutdownFlag;
      this.strategy = strategy;
      this.ended = ended;
    }
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var makePubSubUnsafe = /* @__PURE__ */ __name((pubsub, subscribers, scope3, shutdownHook, shutdownFlag, strategy, ended) => new PubSubImpl(pubsub, subscribers, scope3, shutdownHook, shutdownFlag, strategy, ended), "makePubSubUnsafe");
  var DroppingStrategy = class {
    static {
      __name(this, "DroppingStrategy");
    }
    get shutdown() {
      return void_3;
    }
    handleSurplus(_pubsub, _subscribers, _elements, _isShutdown) {
      return succeed7(false);
    }
    onPubSubEmptySpaceUnsafe(_pubsub, _subscribers) {
    }
    completePollersUnsafe(pubsub, subscribers, subscription, pollers) {
      return strategyCompletePollersUnsafe(this, pubsub, subscribers, subscription, pollers);
    }
    completeSubscribersUnsafe(pubsub, subscribers) {
      return strategyCompleteSubscribersUnsafe(this, pubsub, subscribers);
    }
  };
  var strategyCompletePollersUnsafe = /* @__PURE__ */ __name((strategy, pubsub, subscribers, subscription, pollers) => {
    let keepPolling = true;
    while (keepPolling && !subscription.isEmpty()) {
      const poller = take(pollers);
      if (poller === Empty) {
        removeSubscribers(subscribers, subscription, pollers);
        if (pollers.length === 0) {
          keepPolling = false;
        } else {
          addSubscribers(subscribers, subscription, pollers);
        }
      } else {
        const pollResult = subscription.poll();
        if (pollResult === Empty) {
          prepend2(pollers, poller);
        } else {
          doneUnsafe(poller, succeed4(pollResult));
          strategy.onPubSubEmptySpaceUnsafe(pubsub, subscribers);
        }
      }
    }
  }, "strategyCompletePollersUnsafe");
  var strategyCompleteSubscribersUnsafe = /* @__PURE__ */ __name((strategy, pubsub, subscribers) => {
    for (const [subscription, pollersSet] of subscribers) {
      for (const pollers of pollersSet) {
        strategy.completePollersUnsafe(pubsub, subscribers, subscription, pollers);
      }
    }
  }, "strategyCompleteSubscribersUnsafe");
  var ReplayBuffer = class {
    static {
      __name(this, "ReplayBuffer");
    }
    capacity;
    head = {
      value: AbsentValue,
      index: 0,
      next: null
    };
    tail = this.head;
    slideValues = [];
    size = 0;
    index = 0;
    publisherIndex = 0;
    constructor(capacity) {
      this.capacity = capacity;
    }
    slide(value, publisherIndex) {
      this.slideValues[this.index % this.capacity] = {
        value,
        index: publisherIndex
      };
      this.index++;
    }
    offer(a) {
      const index = this.publisherIndex++;
      this.tail.value = a;
      this.tail.index = index;
      this.tail.next = {
        value: AbsentValue,
        index: 0,
        next: null
      };
      this.tail = this.tail.next;
      if (this.size === this.capacity) {
        this.head = this.head.next;
      } else {
        this.size += 1;
      }
      return index;
    }
    offerAll(as3) {
      for (const a of as3) {
        this.offer(a);
      }
    }
  };
  var ReplayWindowImpl = class {
    static {
      __name(this, "ReplayWindowImpl");
    }
    buffer;
    values;
    index = 0;
    remaining;
    slideIndex;
    newestIndex = -1;
    constructor(buffer2) {
      this.buffer = buffer2;
      this.remaining = buffer2.size;
      this.slideIndex = buffer2.index;
      this.values = new Array(this.remaining);
      let node = buffer2.head;
      for (let i = 0; i < this.remaining; i++) {
        this.values[i] = node.value;
        this.newestIndex = node.index;
        node = node.next;
      }
    }
    close() {
      this.values.length = 0;
      this.remaining = 0;
    }
    sync() {
      const slides = this.buffer.index - this.slideIndex;
      if (slides === 0 || this.remaining === 0) {
        return;
      }
      const count = Math.min(slides, this.buffer.capacity);
      const start3 = this.buffer.index - count;
      for (let i = 0; i < count; i++) {
        const entry = this.buffer.slideValues[(start3 + i) % this.buffer.capacity];
        if (entry.index > this.newestIndex) {
          this.index = (this.index + 1) % this.values.length;
          this.values[(this.index + this.remaining - 1) % this.values.length] = entry.value;
          this.newestIndex = entry.index;
        }
      }
      this.slideIndex = this.buffer.index;
    }
    take() {
      if (this.remaining === 0) {
        return void 0;
      }
      this.sync();
      const value = this.values[this.index];
      this.values[this.index] = AbsentValue;
      this.index = (this.index + 1) % this.values.length;
      this.remaining--;
      if (this.remaining === 0) {
        this.close();
      }
      return value;
    }
    takeN(n) {
      n = normalize(n);
      const len = Math.min(n, this.remaining);
      const items = new Array(len);
      for (let i = 0; i < len; i++) {
        items[i] = this.take();
      }
      return items;
    }
    takeAll() {
      return this.takeN(this.remaining);
    }
  };
  var emptyReplayWindow = {
    remaining: 0,
    take: /* @__PURE__ */ __name(() => void 0, "take"),
    takeN: /* @__PURE__ */ __name(() => [], "takeN"),
    takeAll: /* @__PURE__ */ __name(() => [], "takeAll"),
    close: /* @__PURE__ */ __name(() => void 0, "close")
  };

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Queue.js
  var TypeId16 = "~effect/Queue";
  var EnqueueTypeId = "~effect/Queue/Enqueue";
  var DequeueTypeId = "~effect/Queue/Dequeue";
  var variance = {
    _A: identity,
    _E: identity
  };
  var QueueProto = {
    [TypeId16]: variance,
    [EnqueueTypeId]: variance,
    [DequeueTypeId]: variance,
    ...PipeInspectableProto,
    toJSON() {
      return {
        _id: "effect/Queue",
        state: this.state._tag,
        size: sizeUnsafe(this)
      };
    }
  };
  var make14 = /* @__PURE__ */ __name((options) => withFiber((fiber2) => {
    const self = Object.create(QueueProto);
    self.dispatcher = fiber2.currentDispatcher;
    self.capacity = options?.capacity ?? Number.POSITIVE_INFINITY;
    self.strategy = options?.strategy ?? "suspend";
    self.messages = make11();
    self.scheduleRunning = false;
    self.state = {
      _tag: "Open",
      takers: /* @__PURE__ */ new Set(),
      offers: /* @__PURE__ */ new Set(),
      awaiters: /* @__PURE__ */ new Set()
    };
    return succeed3(self);
  }), "make");
  var bounded = /* @__PURE__ */ __name((capacity) => make14({
    capacity
  }), "bounded");
  var unbounded2 = /* @__PURE__ */ __name(() => make14(), "unbounded");
  var offer = /* @__PURE__ */ dual(2, (self, message2) => suspend(() => offerUnsafe(self, message2) ? exitTrue : self.state._tag === "Open" && self.strategy === "suspend" ? offerOrWait(self, message2) : exitFalse));
  var offerUnsafe = /* @__PURE__ */ __name((self, message2) => {
    if (self.state._tag !== "Open") {
      return false;
    } else if (self.messages.length >= self.capacity) {
      if (self.strategy === "sliding") {
        take(self.messages);
        append2(self.messages, message2);
        return true;
      } else if (self.capacity <= 0 && self.state.takers.size > 0) {
        append2(self.messages, message2);
        releaseTakers(self);
        return true;
      }
      return false;
    }
    append2(self.messages, message2);
    scheduleReleaseTaker(self);
    return true;
  }, "offerUnsafe");
  var failCause5 = /* @__PURE__ */ dual(2, (self, cause) => sync(() => failCauseUnsafe(self, cause)));
  var failCauseUnsafe = /* @__PURE__ */ __name((self, cause) => {
    if (self.state._tag !== "Open") {
      return false;
    }
    const exit3 = exitFailCause(cause);
    const fail7 = exitZipRight(exit3, exitFailDone);
    if (self.state.offers.size === 0 && self.messages.length === 0) {
      finalize(self, fail7);
      return true;
    }
    self.state = {
      ...self.state,
      _tag: "Closing",
      exit: fail7
    };
    scheduleReleaseTaker(self);
    return true;
  }, "failCauseUnsafe");
  var shutdown2 = /* @__PURE__ */ __name((self) => sync(() => shutdownUnsafe(self)), "shutdown");
  var shutdownUnsafe = /* @__PURE__ */ __name((self) => {
    if (self.state._tag === "Done") {
      return false;
    }
    clear2(self.messages);
    const offers = self.state.offers;
    finalize(self, self.state._tag === "Open" ? exitInterrupt2 : self.state.exit);
    for (const entry of offers) {
      resumeUnoffered(entry);
    }
    return true;
  }, "shutdownUnsafe");
  var takeAll3 = /* @__PURE__ */ __name((self) => takeBetween(self, 1, Number.POSITIVE_INFINITY), "takeAll");
  var takeBetween = /* @__PURE__ */ dual(3, (self, min3, max3) => {
    min3 = normalize(min3);
    max3 = normalize(max3);
    return suspendTake(() => takeBetweenUnsafe(self, min3, max3) ?? andThen(awaitTake(self, () => canTake(self, min3)), takeBetween(self, min3, max3)));
  });
  var take3 = /* @__PURE__ */ __name((self) => suspendTake(() => takeUnsafe(self) ?? andThen(awaitTake(self, () => canTake(self, 1)), take3(self))), "take");
  var takeUnsafe = /* @__PURE__ */ __name((self) => {
    if (self.state._tag === "Done") {
      return self.state.exit;
    }
    if (self.messages.length > 0) {
      const message2 = take(self.messages);
      releaseCapacity(self);
      return exitSucceed(message2);
    } else if (self.capacity <= 0 && self.state.offers.size > 0) {
      const message2 = takeOfferUnsafe(self.state.offers);
      releaseCapacity(self);
      return exitSucceed(message2);
    }
    return void 0;
  }, "takeUnsafe");
  var sizeUnsafe = /* @__PURE__ */ __name((self) => self.state._tag === "Done" ? 0 : self.messages.length, "sizeUnsafe");
  var exitFalse = /* @__PURE__ */ exitSucceed(false);
  var exitTrue = /* @__PURE__ */ exitSucceed(true);
  var exitFailDone = /* @__PURE__ */ exitFail(/* @__PURE__ */ Done());
  var exitInterrupt2 = /* @__PURE__ */ exitInterrupt();
  var releaseTakers = /* @__PURE__ */ __name((self) => {
    if (self.state._tag === "Done" || self.state.takers.size === 0) {
      return;
    }
    for (const taker of self.state.takers) {
      if (!taker.ready()) continue;
      self.state.takers.delete(taker);
      taker.resume(exitVoid);
      if (self.messages.length === 0) {
        break;
      }
    }
  }, "releaseTakers");
  var scheduleReleaseTaker = /* @__PURE__ */ __name((self) => {
    if (self.scheduleRunning || self.state._tag === "Done" || self.state.takers.size === 0) {
      return;
    }
    self.scheduleRunning = true;
    self.dispatcher.scheduleTask(() => {
      self.scheduleRunning = false;
      releaseTakers(self);
    }, 0);
  }, "scheduleReleaseTaker");
  var takeBetweenUnsafe = /* @__PURE__ */ __name((self, min3, max3) => {
    if (self.state._tag === "Done") {
      return self.state.exit;
    } else if (max3 <= 0 || min3 <= 0) {
      return exitSucceed([]);
    } else if (!canTake(self, min3)) {
      return void 0;
    }
    const messages = self.messages.length > 0 ? takeN(self.messages, max3) : [takeOfferUnsafe(self.state.offers)];
    releaseCapacity(self);
    return exitSucceed(messages);
  }, "takeBetweenUnsafe");
  var suspendTake = /* @__PURE__ */ makePrimitive({
    op: "QueueTake",
    [evaluate](fiber2) {
      const effect2 = this[args]();
      return isExit(effect2) ? effect2[evaluate](fiber2) : effect2;
    }
  });
  var canTake = /* @__PURE__ */ __name((self, min3) => self.messages.length >= (self.state._tag === "Closing" ? 1 : Math.min(min3, self.capacity || 1)) || self.capacity <= 0 && self.state._tag !== "Done" && self.state.offers.size > 0, "canTake");
  var awaitTake = /* @__PURE__ */ __name((self, ready) => callback((resume) => {
    if (self.state._tag === "Done") return resume(self.state.exit);
    if (ready()) return resume(exitVoid);
    const taker = {
      ready,
      resume
    };
    self.state.takers.add(taker);
    return sync(() => {
      if (self.state._tag !== "Done") self.state.takers.delete(taker);
    });
  }), "awaitTake");
  var offerOrWait = /* @__PURE__ */ __name((self, message2) => callback((resume) => offerUnsafe(self, message2) ? resume(exitTrue) : waitToOffer(self, {
    _tag: "Single",
    message: message2,
    resume
  })), "offerOrWait");
  var waitToOffer = /* @__PURE__ */ __name((self, entry) => {
    if (self.state._tag !== "Open") return resumeUnoffered(entry);
    const offers = self.state.offers;
    offers.add(entry);
    return sync(() => {
      if (self.state._tag === "Done") return;
      offers.delete(entry);
      if (self.state._tag === "Closing" && offers.size === 0 && self.messages.length === 0) {
        finalize(self, self.state.exit);
      }
    });
  }, "waitToOffer");
  var resumeUnoffered = /* @__PURE__ */ __name((entry) => entry._tag === "Single" ? entry.resume(exitFalse) : entry.resume(exitSucceed(entry.remaining.slice(entry.offset))), "resumeUnoffered");
  var takeOfferUnsafe = /* @__PURE__ */ __name((offers) => {
    const entry = offers.values().next().value;
    if (entry._tag === "Single") {
      offers.delete(entry);
      entry.resume(exitTrue);
      return entry.message;
    }
    const message2 = entry.remaining[entry.offset++];
    if (entry.offset === entry.remaining.length) {
      offers.delete(entry);
      entry.resume(exitSucceed([]));
    }
    return message2;
  }, "takeOfferUnsafe");
  var releaseCapacity = /* @__PURE__ */ __name((self) => {
    if (self.state._tag === "Done") {
      return isDoneCause(self.state.exit.cause);
    } else if (self.state.offers.size === 0) {
      if (self.state._tag === "Closing" && self.messages.length === 0) {
        finalize(self, self.state.exit);
        return isDoneCause(self.state.exit.cause);
      }
      return false;
    }
    for (const entry of self.state.offers) {
      let n = self.capacity - self.messages.length;
      if (n <= 0) break;
      else if (entry._tag === "Single") {
        append2(self.messages, entry.message);
        self.state.offers.delete(entry);
        entry.resume(exitTrue);
      } else {
        for (; entry.offset < entry.remaining.length; entry.offset++) {
          if (n === 0) return false;
          append2(self.messages, entry.remaining[entry.offset]);
          n--;
        }
        self.state.offers.delete(entry);
        entry.resume(exitSucceed([]));
      }
      const state = self.state;
      if (state._tag === "Done") {
        return isDoneCause(state.exit.cause);
      }
    }
    return false;
  }, "releaseCapacity");
  var finalize = /* @__PURE__ */ __name((self, exit3) => {
    if (self.state._tag === "Done") {
      return;
    }
    const openState = self.state;
    self.state = {
      _tag: "Done",
      exit: exit3
    };
    for (const taker of openState.takers) {
      taker.resume(exit3);
    }
    openState.takers.clear();
    for (const awaiter of openState.awaiters) {
      awaiter(exit3);
    }
    openState.awaiters.clear();
  }, "finalize");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Semaphore.js
  var makeUnsafe5 = /* @__PURE__ */ __name((permits) => new SemaphoreImpl(permits), "makeUnsafe");
  var waitForPermits = /* @__PURE__ */ __name((self, n, effect2) => callback((resume) => {
    if (self.free >= n) return resume(effect2);
    const observer = /* @__PURE__ */ __name(() => {
      if (self.free < n) return;
      self.waiters.delete(observer);
      resume(effect2);
    }, "observer");
    self.waiters.add(observer);
    return sync(() => {
      self.waiters.delete(observer);
    });
  }), "waitForPermits");
  var SemaphoreImpl = class {
    static {
      __name(this, "SemaphoreImpl");
    }
    waiters = /* @__PURE__ */ new Set();
    taken = 0;
    permits;
    constructor(permits) {
      this.permits = permits;
    }
    get free() {
      return this.permits - this.taken;
    }
    take(n) {
      const take4 = suspend(() => {
        if (this.free < n) {
          return waitForPermits(this, n, take4);
        }
        this.taken += n;
        return succeed3(n);
      });
      return take4;
    }
    takeIfAvailable(n) {
      return suspend(() => {
        if (this.free < n) return succeed3(false);
        this.taken += n;
        return succeed3(true);
      });
    }
    releaseUnsafe(fiber2, n) {
      this.taken -= n;
      if (this.waiters.size > 0) {
        fiber2.currentDispatcher.scheduleTask(() => {
          for (const observer of this.waiters) {
            if (this.free <= 0) break;
            observer();
          }
        }, 0);
      }
      return this.free;
    }
    resize(permits) {
      return withFiber((fiber2) => {
        this.permits = permits;
        if (this.free < 0) return void_;
        this.releaseUnsafe(fiber2, 0);
        return void_;
      });
    }
    release(n) {
      return withFiber((fiber2) => succeed3(this.releaseUnsafe(fiber2, n)));
    }
    get releaseAll() {
      return withFiber((fiber2) => succeed3(this.releaseUnsafe(fiber2, this.taken)));
    }
    withPermits(n) {
      return (self) => uninterruptibleMask((restore) => {
        const acquire = suspend(() => {
          if (this.free < n) {
            const wait = waitForPermits(this, n, void_);
            return flatMap2(restore(wait), () => acquire);
          }
          this.taken += n;
          return onExitPrimitive(restore(self), () => {
            this.releaseUnsafe(getCurrentFiber(), n);
            return void 0;
          }, true);
        });
        return acquire;
      });
    }
    withPermit = /* @__PURE__ */ this.withPermits(1);
    withPermitsIfAvailable(n) {
      return (self) => uninterruptibleMask((restore) => {
        if (this.free < n) return succeedNone;
        this.taken += n;
        return onExitPrimitive(restore(asSome(self)), () => {
          this.releaseUnsafe(getCurrentFiber(), n);
          return void 0;
        }, true);
      });
    }
  };
  var make15 = /* @__PURE__ */ __name((permits) => sync(() => new SemaphoreImpl(permits)), "make");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Channel.js
  var TypeId17 = "~effect/Channel";
  var isChannel = /* @__PURE__ */ __name((u) => hasProperty(u, TypeId17), "isChannel");
  var ChannelProto = {
    [TypeId17]: {
      _Env: identity,
      _InErr: identity,
      _InElem: identity,
      _OutErr: identity,
      _OutElem: identity
    },
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var fromTransform = /* @__PURE__ */ __name((transform3) => {
    const self = Object.create(ChannelProto);
    self.transform = (upstream, scope3) => catchCause2(transform3(upstream, scope3), (cause) => succeed7(failCause4(cause)));
    return self;
  }, "fromTransform");
  var transformPull = /* @__PURE__ */ __name((self, f) => fromTransform((upstream, scope3) => flatMap3(toTransform(self)(upstream, scope3), (pull) => f(pull, scope3))), "transformPull");
  var fromPull = /* @__PURE__ */ __name((effect2) => fromTransform((_, __) => effect2), "fromPull");
  var fromTransformBracket = /* @__PURE__ */ __name((f) => fromTransform(fnUntraced2(function* (upstream, scope3) {
    const closableScope = forkUnsafe2(scope3);
    const onCause = /* @__PURE__ */ __name((cause) => close(closableScope, doneExitFromCause(cause)), "onCause");
    const pull = yield* onError2(f(upstream, scope3, closableScope), onCause);
    return onError2(pull, onCause);
  })), "fromTransformBracket");
  var toTransform = /* @__PURE__ */ __name((channel) => channel.transform, "toTransform");
  var fromQueueArray = /* @__PURE__ */ __name((queue) => fromPull(succeed7(takeAll3(queue))), "fromQueueArray");
  var fromSubscriptionArray = /* @__PURE__ */ __name((subscription) => fromPull(succeed7(onInterrupt2(takeAll2(subscription), () => done3()))), "fromSubscriptionArray");
  var fromPubSubArray = /* @__PURE__ */ __name((pubsub) => unwrap(map5(subscribe(pubsub), fromSubscriptionArray)), "fromPubSubArray");
  var map7 = /* @__PURE__ */ dual(2, (self, f) => transformPull(self, (pull) => sync2(() => {
    let i = 0;
    return map5(pull, (o) => f(o, i++));
  })));
  var concurrencyIsSequential = /* @__PURE__ */ __name((concurrency) => concurrency === void 0 || concurrency !== "unbounded" && concurrency <= 1, "concurrencyIsSequential");
  var mapEffect = /* @__PURE__ */ dual((args2) => isChannel(args2[0]), (self, f, options) => concurrencyIsSequential(options?.concurrency) ? mapEffectSequential(self, f) : mapEffectConcurrent(self, f, options));
  var mapEffectSequential = /* @__PURE__ */ __name((self, f) => fromTransform((upstream, scope3) => {
    let i = 0;
    return map5(toTransform(self)(upstream, scope3), flatMap3((o) => f(o, i++)));
  }), "mapEffectSequential");
  var mapEffectConcurrent = /* @__PURE__ */ __name((self, f, options) => fromTransformBracket(fnUntraced2(function* (upstream, scope3, forkedScope) {
    let i = 0;
    const pull = yield* toTransform(self)(upstream, scope3);
    const concurrencyN = options.concurrency === "unbounded" ? Number.MAX_SAFE_INTEGER : options.concurrency;
    const queue = yield* bounded(0);
    yield* addFinalizer2(forkedScope, shutdown2(queue));
    const runFork3 = runForkWith2(yield* context2());
    const trackFiber = runIn(forkedScope);
    if (options.unordered) {
      const semaphore = makeUnsafe5(concurrencyN);
      const release = constant(semaphore.release(1));
      const handle = matchCauseEffect2({
        onFailure: /* @__PURE__ */ __name((cause) => flatMap3(failCause5(queue, cause), release), "onFailure"),
        onSuccess: /* @__PURE__ */ __name((value) => flatMap3(offer(queue, value), release), "onSuccess")
      });
      yield* semaphore.take(1).pipe(flatMap3(() => pull), flatMap3((value) => {
        const index = i++;
        trackFiber(runFork3(handle(suspend2(() => f(value, index)))));
        return void_3;
      }), forever3({
        disableYield: true
      }), catchCause2((cause) => semaphore.withPermits(concurrencyN - 1)(failCause5(queue, cause))), forkIn2(forkedScope));
    } else {
      const effects = yield* bounded(concurrencyN - 2);
      yield* addFinalizer2(forkedScope, shutdown2(effects));
      yield* take3(effects).pipe(flatten4, flatMap3((value) => offer(queue, value)), forever3({
        disableYield: true
      }), catchCause2((cause) => failCause5(queue, cause)), forkIn2(forkedScope));
      let errorCause;
      const onExit4 = /* @__PURE__ */ __name((exit3) => {
        if (exit3._tag === "Success") return;
        errorCause = exit3.cause;
        failCauseUnsafe(queue, exit3.cause);
      }, "onExit");
      yield* pull.pipe(flatMap3((value) => {
        if (errorCause) return failCause4(errorCause);
        const index = i++;
        const fiber2 = runFork3(suspend2(() => f(value, index)));
        trackFiber(fiber2);
        fiber2.addObserver(onExit4);
        return offer(effects, join(fiber2));
      }), forever3({
        disableYield: true
      }), catchCause2((cause) => offer(effects, failCause2(cause)).pipe(andThen2(failCause5(effects, cause)))), forkIn2(forkedScope));
    }
    return take3(queue);
  })), "mapEffectConcurrent");
  var flattenArray = /* @__PURE__ */ __name((self) => transformPull(self, (pull) => {
    let array2;
    let index = 0;
    const pump = suspend2(/* @__PURE__ */ __name(function loop() {
      if (array2 === void 0) {
        return flatMap3(pull, (array_) => {
          switch (array_.length) {
            case 0:
              return loop();
            case 1:
              return succeed7(array_[0]);
            default: {
              array2 = array_;
              return succeed7(array_[index++]);
            }
          }
        });
      }
      const next = array2[index++];
      if (index >= array2.length) {
        array2 = void 0;
        index = 0;
      }
      return succeed7(next);
    }, "loop"));
    return succeed7(pump);
  }), "flattenArray");
  var schedule2 = /* @__PURE__ */ dual(2, (self, schedule4) => transformPull(self, (pull, _scope) => map5(toStepWithSleep(schedule4), (step) => {
    const pullWithStep = tap2(pull, step);
    return pullWithStep;
  })));
  var filterArray = /* @__PURE__ */ dual(2, (self, predicate) => transformPull(self, (pull) => succeed7(flatMap3(pull, /* @__PURE__ */ __name(function loop(arr) {
    const passes = [];
    for (let i = 0; i < arr.length; i++) {
      if (predicate(arr[i])) {
        passes.push(arr[i]);
      }
    }
    return isReadonlyArrayNonEmpty(passes) ? succeed7(passes) : flatMap3(pull, loop);
  }, "loop")))));
  var unwrap = /* @__PURE__ */ __name((channel) => fromTransform((upstream, scope3) => {
    let pull;
    return succeed7(suspend2(() => {
      if (pull) return pull;
      return channel.pipe(provide(scope3), flatMap3((channel2) => toTransform(channel2)(upstream, scope3)), flatMap3((pull_) => pull = pull_));
    }));
  }), "unwrap");
  var runWith = /* @__PURE__ */ __name((self, f, onHalt) => suspend2(() => {
    const scope3 = makeUnsafe3();
    const makePull = toTransform(self)(done3(), scope3);
    return catchDone(flatMap3(makePull, f), onHalt ? onHalt : succeed7).pipe(onExit2((exit3) => close(scope3, exit3)));
  }), "runWith");
  var runDrain = /* @__PURE__ */ __name((self) => runWith(self, (pull) => forever3(pull, {
    disableYield: true
  })), "runDrain");
  var runForEach = /* @__PURE__ */ dual(2, (self, f) => runWith(self, (pull) => forever3(flatMap3(pull, f), {
    disableYield: true
  })));

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/internal/stream.js
  var TypeId18 = "~effect/Stream";
  var streamVariance = {
    _R: identity,
    _E: identity,
    _A: identity
  };
  var Stream = /* @__PURE__ */ __name(function(channel) {
    this.channel = channel;
  }, "Stream");
  Stream.prototype = {
    [TypeId18]: streamVariance,
    pipe() {
      return pipeArguments(this, arguments);
    }
  };
  var fromChannel = /* @__PURE__ */ __name((channel) => new Stream(channel), "fromChannel");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Stream.js
  var TypeId19 = "~effect/Stream";
  var isStream = /* @__PURE__ */ __name((u) => hasProperty(u, TypeId19), "isStream");
  var fromChannel2 = fromChannel;
  var toChannel = /* @__PURE__ */ __name((stream) => stream.channel, "toChannel");
  var fromQueue = /* @__PURE__ */ __name((queue) => fromChannel2(fromQueueArray(queue)), "fromQueue");
  var fromPubSub = /* @__PURE__ */ __name((pubsub) => fromChannel2(fromPubSubArray(pubsub)), "fromPubSub");
  var mapEffect2 = /* @__PURE__ */ dual((args2) => isStream(args2[0]), (self, f, options) => self.channel.pipe(flattenArray, mapEffect(f, options), map7(of), fromChannel2));
  var schedule3 = /* @__PURE__ */ dual(2, (self, schedule4) => self.channel.pipe(flattenArray, schedule2(schedule4), map7(of), fromChannel2));
  var filter6 = /* @__PURE__ */ dual(2, (self, predicate) => fromChannel2(filterArray(toChannel(self), predicate)));
  var runForEach2 = /* @__PURE__ */ dual(2, (self, f) => runForEach(self.channel, (arr) => {
    let i = 0;
    return whileLoop2({
      while: /* @__PURE__ */ __name(() => i < arr.length, "while"),
      body: /* @__PURE__ */ __name(() => f(arr[i++]), "body"),
      step: constVoid
    });
  }));
  var runDrain2 = /* @__PURE__ */ __name((self) => runDrain(self.channel), "runDrain");

  // packages/runtime/src/game/flash/contract/Callback.ts
  var Callback = Union2([
    Struct({ type: Literal2("connection"), status: String4 }),
    Struct({ type: Literal2("debug"), message: String4 }),
    Struct({
      type: Literal2("extension-packet"),
      raw: String4
    }),
    Struct({ type: Literal2("client-packet"), raw: String4 }),
    Struct({ type: Literal2("server-packet"), raw: String4 })
  ]);
  var decodeCallback = decodeUnknownOption2(Callback);

  // packages/runtime/src/game/flash/contract/Packet.ts
  var PacketDirection = Literals([
    "client",
    "server",
    "extension"
  ]);
  var PacketEncoding = Literals(["string", "json"]);
  var ClientPacket = Struct({
    command: String4,
    direction: Literal2("client"),
    params: ArraySchema(String4),
    raw: String4,
    encoding: PacketEncoding
  });
  var ServerPacket = Struct({
    command: String4,
    data: Unknown2,
    direction: Literal2("server"),
    raw: String4,
    encoding: PacketEncoding
  });
  var ExtensionPacket = Struct({
    command: String4,
    data: Unknown2,
    direction: Literal2("extension"),
    raw: String4,
    encoding: PacketEncoding
  });
  var Packet = Union2([
    ClientPacket,
    ServerPacket,
    ExtensionPacket
  ]);
  var packetData = /* @__PURE__ */ __name((packet) => packet.direction === "client" ? packet.params : packet.data, "packetData");
  var matchesPacket = /* @__PURE__ */ __name((packet, selector) => (selector?.command === void 0 || selector.command === packet.command) && (selector?.direction === void 0 || selector.direction === packet.direction) && (selector?.encoding === void 0 || selector.encoding === packet.encoding) && (selector?.predicate === void 0 || selector.predicate(packet)), "matchesPacket");

  // packages/runtime/src/game/flash/contract/Coercion.ts
  var invalid3 = /* @__PURE__ */ __name((value, message2) => fail5(
    new InvalidValue({ message: message2 }, value, { reportInput: true })
  ), "invalid");
  var NumberInput = Union2([Number6, String4]);
  var WireNumber = NumberInput.pipe(
    decodeTo2(
      Number6,
      transformEffect2({
        decode: /* @__PURE__ */ __name((value) => {
          if (typeof value === "number") {
            return Number.isFinite(value) ? succeed7(value) : invalid3(value, "Expected a finite number");
          }
          const trimmed = value.trim();
          if (trimmed === "") {
            return invalid3(value, "Expected a numeric string");
          }
          const decoded = Number(trimmed);
          return Number.isFinite(decoded) ? succeed7(decoded) : invalid3(value, "Expected a numeric string");
        }, "decode"),
        encode: succeed7
      })
    )
  );
  var WireInt = WireNumber.pipe(
    decodeTo2(
      Int,
      transformEffect2({
        decode: /* @__PURE__ */ __name((value) => succeed7(Math.trunc(value)), "decode"),
        encode: succeed7
      })
    )
  );
  var PositiveWireInt = WireInt.check(isGreaterThanOrEqualTo2(1));
  var NonNegativeWireInt = WireInt.check(
    isGreaterThanOrEqualTo2(0)
  );
  var BooleanInput = Union2([
    Boolean3,
    Number6,
    String4
  ]);
  var WireBoolean = BooleanInput.pipe(
    decodeTo2(
      Boolean3,
      transformEffect2({
        decode: /* @__PURE__ */ __name((value) => {
          if (typeof value === "boolean") return succeed7(value);
          if (value === 1 || value === "1" || value === "true") {
            return succeed7(true);
          }
          if (value === 0 || value === "0" || value === "false") {
            return succeed7(false);
          }
          return invalid3(value, "Expected a boolean or boolean wire value");
        }, "decode"),
        encode: succeed7
      })
    )
  );
  var UnknownRecord = Record(String4, Unknown2);

  // packages/runtime/src/game/flash/protocol/PacketCodec.ts
  var decodeClientPacket = decodeUnknownOption2(ClientPacket);
  var decodeServerPacket = decodeUnknownOption2(ServerPacket);
  var decodeExtensionPacket = decodeUnknownOption2(ExtensionPacket);
  var decodeRecord = decodeUnknownOption2(UnknownRecord);
  var decodeExtensionEnvelope = decodeUnknownOption2(
    Struct({
      type: Literals(["str", "json"]),
      dataObj: Unknown2
    })
  );
  var decodeWrappedServer = decodeUnknownOption2(
    Struct({
      t: String4,
      b: Struct({ o: Unknown2 })
    })
  );
  var logPrefixes = [
    "[Sending - JSON]: ",
    "[Sending - STR]: ",
    "[ RECEIVED ]: ",
    "[Sending]: "
  ];
  var stripLogEnvelope = /* @__PURE__ */ __name((raw) => {
    const trimmed = raw.trim();
    const prefix = logPrefixes.find((candidate) => trimmed.startsWith(candidate));
    const payload = prefix === void 0 ? trimmed : trimmed.slice(prefix.length);
    return payload.replace(/, \(len: \d+\)$/u, "").trim();
  }, "stripLogEnvelope");
  var stringPacket = /* @__PURE__ */ __name((payload) => {
    if (!payload.startsWith("%xt%")) return void 0;
    const params = payload.split("%").filter(Boolean);
    const command = params[params[1] === "zm" ? 2 : 1];
    return command === void 0 || command === "" ? void 0 : { command, params };
  }, "stringPacket");
  var parseJson2 = /* @__PURE__ */ __name((raw) => {
    try {
      return some2(JSON.parse(raw));
    } catch {
      return none2();
    }
  }, "parseJson");
  var isUnsupportedPacketEnvelope = /* @__PURE__ */ __name((raw) => {
    const payload = stripLogEnvelope(raw);
    if (payload.startsWith("<")) return true;
    const parsed = parseJson2(payload);
    if (isNone2(parsed)) return false;
    const record3 = decodeRecord(parsed.value);
    return isSome2(record3) && record3.value["type"] === "xml";
  }, "isUnsupportedPacketEnvelope");
  var parseClientPacket = /* @__PURE__ */ __name((raw) => {
    const payload = stripLogEnvelope(raw);
    const packet = stringPacket(payload);
    return packet === void 0 ? none2() : decodeClientPacket({
      command: packet.command,
      direction: "client",
      params: packet.params,
      raw,
      encoding: "string"
    });
  }, "parseClientPacket");
  var parseServerPacket = /* @__PURE__ */ __name((raw) => {
    const payload = stripLogEnvelope(raw);
    const packet = stringPacket(payload);
    if (packet !== void 0) {
      return decodeServerPacket({
        command: packet.command,
        data: packet.params,
        direction: "server",
        raw,
        encoding: "string"
      });
    }
    return flatMap(parseJson2(payload), (value) => {
      const wrapped = decodeWrappedServer(value);
      if (isSome2(wrapped)) {
        const decodedData = decodeRecord(wrapped.value.b.o);
        if (isNone2(decodedData)) return none2();
        const data = decodedData.value;
        const command = data["cmd"];
        return typeof command !== "string" || command === "" ? none2() : decodeServerPacket({
          command,
          data,
          direction: "server",
          raw,
          encoding: "json"
        });
      }
      return flatMap(decodeRecord(value), (data) => {
        const command = data["cmd"];
        return typeof command === "string" && command !== "" ? decodeServerPacket({
          command,
          data,
          direction: "server",
          raw,
          encoding: "json"
        }) : none2();
      });
    });
  }, "parseServerPacket");
  var parseExtensionPacket = /* @__PURE__ */ __name((raw) => {
    const payload = stripLogEnvelope(raw);
    return flatMap(
      parseJson2(payload),
      (value) => flatMap(decodeExtensionEnvelope(value), (envelope) => {
        if (envelope.type === "str") {
          if (!Array.isArray(envelope.dataObj)) return none2();
          const command = envelope.dataObj[0];
          return typeof command === "string" ? decodeExtensionPacket({
            command,
            data: envelope.dataObj,
            direction: "extension",
            raw,
            encoding: "string"
          }) : none2();
        }
        const data = decodeRecord(envelope.dataObj);
        if (isSome2(data)) {
          const command = data.value["cmd"];
          return typeof command === "string" && command !== "" ? decodeExtensionPacket({
            command,
            data: data.value,
            direction: "extension",
            raw,
            encoding: "json"
          }) : none2();
        }
        return none2();
      })
    );
  }, "parseExtensionPacket");
  var parsePacket = /* @__PURE__ */ __name((direction, raw) => {
    switch (direction) {
      case "client":
        return parseClientPacket(raw);
      case "server":
        return parseServerPacket(raw);
      case "extension":
        return parseExtensionPacket(raw);
    }
  }, "parsePacket");

  // packages/runtime/src/game/flash/bridge/Gateway.ts
  var clientPacketTypes = {
    json: "json",
    string: "str",
    xml: "xml"
  };
  var serverPacketTypes = {
    json: "Json",
    string: "String"
  };
  var packetInput = /* @__PURE__ */ __name((callback3) => {
    switch (callback3.type) {
      case "client-packet":
        return { direction: "client", raw: callback3.raw };
      case "extension-packet":
        return { direction: "extension", raw: callback3.raw };
      case "server-packet":
        return { direction: "server", raw: callback3.raw };
      default:
        return void 0;
    }
  }, "packetInput");
  var runtimeEvent = /* @__PURE__ */ __name((callback3) => {
    switch (callback3.type) {
      case "connection":
        return { type: "connection", status: callback3.status };
      case "debug":
        return { type: "debug", message: callback3.message };
      default:
        return void 0;
    }
  }, "runtimeEvent");
  var makeGateway = /* @__PURE__ */ __name((target) => gen2(function* () {
    const resolvedTarget = target ?? window;
    const bridge = yield* Bridge;
    const diagnosticSink = yield* DiagnosticSink;
    const scope3 = yield* scope2;
    const callbacks = yield* unbounded2();
    const events = yield* unbounded();
    const packets = yield* unbounded();
    const rawPackets = yield* unbounded();
    let started = false;
    yield* addFinalizer3(
      () => all2(
        [
          shutdown2(callbacks),
          shutdown(events),
          shutdown(packets),
          shutdown(rawPackets)
        ],
        { discard: true }
      )
    );
    const reportDiagnosticUnsafe = /* @__PURE__ */ __name((phase, operation, cause, args2) => {
      diagnosticSink.report(phase, operation, cause, args2);
    }, "reportDiagnosticUnsafe");
    const enqueue = /* @__PURE__ */ __name((operation, delivery) => {
      if (!offerUnsafe(callbacks, delivery)) {
        reportDiagnosticUnsafe(
          "callback-dispatch",
          operation,
          new Error("Callback queue is unavailable")
        );
      }
    }, "enqueue");
    const offer2 = /* @__PURE__ */ __name((operation, value) => {
      const decoded = decodeCallback(value);
      if (isNone2(decoded)) {
        reportDiagnosticUnsafe(
          "callback-decode",
          operation,
          new Error("Invalid callback input"),
          [value]
        );
        return;
      }
      enqueue(operation, decoded.value);
    }, "offer");
    const owned = {
      onConnection: /* @__PURE__ */ __name((status) => offer2("onConnection", { type: "connection", status }), "onConnection"),
      onDebug: /* @__PURE__ */ __name((message2) => offer2("onDebug", { type: "debug", message: message2 }), "onDebug"),
      onExtensionResponse: /* @__PURE__ */ __name((raw) => offer2("onExtensionResponse", { type: "extension-packet", raw }), "onExtensionResponse"),
      onProjectionEvent: /* @__PURE__ */ __name((event) => enqueue("onProjectionEvent", { type: "projection", event }), "onProjectionEvent"),
      packetFromClient: /* @__PURE__ */ __name((raw) => offer2("packetFromClient", { type: "client-packet", raw }), "packetFromClient"),
      packetFromServer: /* @__PURE__ */ __name((raw) => offer2("packetFromServer", { type: "server-packet", raw }), "packetFromServer")
    };
    for (const key of Object.keys(owned)) {
      resolvedTarget[key] = owned[key];
    }
    yield* addFinalizer3(
      () => sync2(() => {
        for (const key of Object.keys(owned)) {
          if (resolvedTarget[key] === owned[key]) delete resolvedTarget[key];
        }
      })
    );
    const dispatch = /* @__PURE__ */ __name((delivery, handle) => gen2(function* () {
      const event = delivery.type === "projection" ? delivery.event : runtimeEvent(delivery);
      if (event !== void 0) {
        yield* handle(event);
        yield* publish(events, event);
        return;
      }
      const input = packetInput(delivery);
      if (input === void 0) return;
      yield* publish(rawPackets, input);
      const packet = parsePacket(input.direction, input.raw);
      if (isNone2(packet)) {
        if (isUnsupportedPacketEnvelope(input.raw)) return;
        yield* sync2(
          () => diagnosticSink.report(
            "packet-decode",
            `${input.direction}:packet`,
            new Error("Malformed packet envelope"),
            [input.raw]
          )
        );
        return;
      }
      yield* publish(packets, packet.value);
    }), "dispatch");
    const start3 = /* @__PURE__ */ __name((handle = () => void_3) => suspend2(() => {
      if (started) return void_3;
      started = true;
      const drain2 = forever3(
        take3(callbacks).pipe(
          flatMap3((delivery) => dispatch(delivery, handle)),
          catchCause2(
            (cause) => sync2(
              () => diagnosticSink.report("callback-dispatch", "drain", cause)
            )
          )
        )
      );
      return forkIn2(drain2, scope3).pipe(asVoid2);
    }), "start");
    const sendToClient = /* @__PURE__ */ __name((packet, encoding = "string") => bridge.invoke(
      "flash.sendClientPacket",
      [packet, clientPacketTypes[encoding]],
      Void2
    ).pipe(map5(isSome2)), "sendToClient");
    const sendToServer = /* @__PURE__ */ __name((packet, encoding = "string") => bridge.invoke(
      "flash.callGameFunction",
      [`sfc.send${serverPacketTypes[encoding]}`, packet],
      String4
    ).pipe(map5(isSome2)), "sendToServer");
    return {
      events: fromPubSub(events),
      packets: fromPubSub(packets),
      rawPackets: fromPubSub(rawPackets),
      sendToClient,
      sendToServer,
      start: start3,
      subscribeEvents: subscribe(events),
      subscribePackets: subscribe(packets)
    };
  }), "makeGateway");
  var Gateway = class extends Service()(
    "lucent/renderer/flash/Gateway",
    { make: makeGateway() }
  ) {
    static {
      __name(this, "Gateway");
    }
  };
  var layer2 = effect(Gateway, Gateway.make);

  // packages/game/src/model.ts
  var normalizeGameText = /* @__PURE__ */ __name((value) => value.trim().toLowerCase(), "normalizeGameText");
  var LiveModel = class {
    static {
      __name(this, "LiveModel");
    }
    #data;
    constructor(data) {
      this.#data = data;
    }
    get modelData() {
      return this.#data;
    }
    /** @internal */
    replaceFrom(model) {
      if (this.#data === model.#data) return;
      for (const key of Reflect.ownKeys(this.#data)) {
        Reflect.deleteProperty(this.#data, key);
      }
      Object.assign(this.#data, model.#data);
    }
    /** @internal */
    snapshot() {
      return { ...this.#data };
    }
  };

  // packages/game/src/aura.ts
  var LiveAura = class extends LiveModel {
    static {
      __name(this, "LiveAura");
    }
    get category() {
      return this.modelData.category;
    }
    /** Full duration in seconds, or `0` when the aura has no timer. */
    get duration() {
      return this.modelData.duration;
    }
    /** Expiry time in epoch milliseconds, or `undefined` when the aura has no timer. The aura stays until the server removes it, even after this time. */
    get expiresAt() {
      return this.modelData.expiresAt;
    }
    get icon() {
      return this.modelData.icon;
    }
    get kind() {
      return this.modelData.kind;
    }
    get name() {
      return this.modelData.name;
    }
    get persistent() {
      return this.modelData.persistent;
    }
    get stack() {
      return this.modelData.stack;
    }
    get value() {
      return this.modelData.value;
    }
    toJSON() {
      return this.snapshot();
    }
  };
  var toLiveAura = /* @__PURE__ */ __name((snapshot) => new LiveAura({ ...snapshot }), "toLiveAura");

  // packages/game/src/enhancement.ts
  var BASIC_HIGH_TIER_MINIMUM_PLAYER_LEVEL = 50;
  var NO_PATTERN_ID = 0;
  var NO_PROC_ID = 0;
  var PATTERN_IDS = {
    absolution: 11,
    anima: 28,
    avarice: 12,
    depths: 23,
    examen: 26,
    fighter: 2,
    forge: 10,
    healer: 7,
    hearty: 32,
    hybrid: 5,
    lament: 30,
    lucky: 9,
    penitence: 29,
    pneuma: 27,
    spellbreaker: 8,
    thief: 3,
    vainglory: 24,
    vim: 25,
    wizard: 6
  };
  var WEAPON_PROC_IDS = {
    acheron: 11,
    arcanaConcerto: 10,
    aweBlast: 3,
    dauntless: 14,
    elysium: 12,
    healthVamp: 4,
    lacerate: 7,
    manaVamp: 5,
    powerwordDie: 6,
    praxis: 13,
    ravenous: 15,
    smite: 8,
    spiralCarve: 2,
    valiance: 9
  };
  var FORGE_SHOP_IDS = {
    cape: 2143,
    helm: 2164,
    weapon: 2142
  };
  var BASIC_ENHANCEMENTS = [
    {
      aweShopId: 635,
      name: "fighter",
      patternId: PATTERN_IDS.fighter,
      shopIds: { high: 768, low: 141 }
    },
    {
      aweShopId: 637,
      name: "thief",
      patternId: PATTERN_IDS.thief,
      shopIds: { high: 767, low: 142 }
    },
    {
      aweShopId: 633,
      name: "hybrid",
      patternId: PATTERN_IDS.hybrid,
      shopIds: { high: 766, low: 143 }
    },
    {
      aweShopId: 636,
      name: "wizard",
      patternId: PATTERN_IDS.wizard,
      shopIds: { high: 765, low: 144 }
    },
    {
      aweShopId: 638,
      name: "healer",
      patternId: PATTERN_IDS.healer,
      shopIds: { high: 762, low: 145 }
    },
    {
      name: "spellbreaker",
      patternId: PATTERN_IDS.spellbreaker,
      shopIds: { high: 764, low: 146 }
    },
    {
      aweShopId: 639,
      name: "lucky",
      patternId: PATTERN_IDS.lucky,
      shopIds: { high: 763, low: 147 }
    }
  ];
  var WEAPON_SPECIALS = [
    {
      aliases: ["scarve", "spiral"],
      family: "awe",
      name: "spiral carve",
      procId: WEAPON_PROC_IDS.spiralCarve
    },
    {
      aliases: ["ablast", "aweblast", "blast"],
      family: "awe",
      name: "awe blast",
      procId: WEAPON_PROC_IDS.aweBlast
    },
    {
      aliases: ["healthvamp", "hvamp", "hp vamp"],
      family: "awe",
      name: "health vamp",
      procId: WEAPON_PROC_IDS.healthVamp
    },
    {
      aliases: ["manavamp", "mvamp", "mp vamp"],
      family: "awe",
      name: "mana vamp",
      procId: WEAPON_PROC_IDS.manaVamp
    },
    {
      aliases: ["powerword", "pwd", "pw die"],
      displayName: "Powerword DIE",
      family: "awe",
      name: "powerword die",
      procId: WEAPON_PROC_IDS.powerwordDie
    },
    {
      aliases: ["lac"],
      family: "forge",
      name: "lacerate",
      patternId: PATTERN_IDS.forge,
      procId: WEAPON_PROC_IDS.lacerate
    },
    {
      family: "forge",
      name: "smite",
      patternId: PATTERN_IDS.forge,
      procId: WEAPON_PROC_IDS.smite
    },
    {
      aliases: ["val", "vali"],
      family: "forge",
      name: "valiance",
      patternId: PATTERN_IDS.forge,
      procId: WEAPON_PROC_IDS.valiance
    },
    {
      aliases: ["arcanas", "arcana concerto", "concerto"],
      family: "forge",
      name: "arcana's concerto",
      patternId: PATTERN_IDS.forge,
      procId: WEAPON_PROC_IDS.arcanaConcerto
    },
    {
      aliases: ["ach"],
      family: "forge",
      name: "acheron",
      patternId: PATTERN_IDS.depths,
      procId: WEAPON_PROC_IDS.acheron
    },
    {
      aliases: ["ely"],
      family: "forge",
      name: "elysium",
      patternId: PATTERN_IDS.wizard,
      procId: WEAPON_PROC_IDS.elysium
    },
    {
      aliases: ["prax"],
      family: "forge",
      name: "praxis",
      patternId: PATTERN_IDS.forge,
      procId: WEAPON_PROC_IDS.praxis
    },
    {
      aliases: ["dtl"],
      family: "forge",
      name: "dauntless",
      patternId: PATTERN_IDS.fighter,
      procId: WEAPON_PROC_IDS.dauntless
    },
    {
      aliases: ["rav"],
      family: "forge",
      name: "ravenous",
      patternId: PATTERN_IDS.forge,
      procId: WEAPON_PROC_IDS.ravenous
    }
  ];
  var FORGE_WEAPON_PATTERNS = [
    { name: "forge", patternId: PATTERN_IDS.forge },
    { name: "depths", patternId: PATTERN_IDS.depths }
  ];
  var CAPE_SPECIALS = [
    { name: "forge", patternId: PATTERN_IDS.forge },
    {
      aliases: ["abso"],
      name: "absolution",
      patternId: PATTERN_IDS.absolution
    },
    { aliases: ["ava"], name: "avarice", patternId: PATTERN_IDS.avarice },
    { name: "vainglory", patternId: PATTERN_IDS.vainglory },
    {
      aliases: ["peni"],
      name: "penitence",
      patternId: PATTERN_IDS.penitence
    },
    { aliases: ["lam"], name: "lament", patternId: PATTERN_IDS.lament }
  ];
  var HELM_ENHANCEMENTS = [
    { name: "vim", patternId: PATTERN_IDS.vim, special: { name: "ether" } },
    {
      name: "examen",
      patternId: PATTERN_IDS.examen,
      special: { name: "ether" }
    },
    {
      name: "pneuma",
      patternId: PATTERN_IDS.pneuma,
      special: { name: "clairvoyance" }
    },
    {
      name: "anima",
      patternId: PATTERN_IDS.anima,
      special: { name: "clairvoyance" }
    },
    {
      inventoryName: "grimskull",
      name: "hearty",
      patternId: PATTERN_IDS.hearty,
      special: { name: "hearty" }
    }
  ];
  var EXTRA_PATTERN_DISPLAY_NAMES = /* @__PURE__ */ new Map([
    [1, "Adventurer"],
    [4, "Armsman"]
  ]);
  var titleCase = /* @__PURE__ */ __name((value) => value.split(" ").map(
    (word) => word === "" ? word : `${word[0].toUpperCase()}${word.slice(1)}`
  ).join(" "), "titleCase");
  var namedValueDisplayName = /* @__PURE__ */ __name((value) => value.displayName ?? titleCase(value.name), "namedValueDisplayName");
  var enhancementPatternDisplayNames = /* @__PURE__ */ __name((patternId) => {
    const basic = BASIC_ENHANCEMENTS.find(
      (entry) => entry.patternId === patternId
    );
    if (basic !== void 0) {
      return [namedValueDisplayName(basic)];
    }
    const weapon = FORGE_WEAPON_PATTERNS.find(
      (entry) => entry.patternId === patternId
    );
    if (weapon !== void 0) {
      return [namedValueDisplayName(weapon)];
    }
    const cape = CAPE_SPECIALS.find((entry) => entry.patternId === patternId);
    if (cape !== void 0) {
      const name = namedValueDisplayName(cape);
      return name === "Forge" ? [name] : ["Forge", name];
    }
    const helm = HELM_ENHANCEMENTS.find((entry) => entry.patternId === patternId);
    if (helm !== void 0) {
      return [
        titleCase("inventoryName" in helm ? helm.inventoryName : helm.name),
        namedValueDisplayName(helm.special)
      ];
    }
    const extra = EXTRA_PATTERN_DISPLAY_NAMES.get(patternId);
    return extra === void 0 ? void 0 : [extra];
  }, "enhancementPatternDisplayNames");
  var getItemEnhancementNames = /* @__PURE__ */ __name((enhancement) => {
    const patternId = enhancement.patternId;
    const [name, patternSpecial] = patternId !== void 0 && patternId > 0 ? enhancementPatternDisplayNames(patternId) ?? [`Pattern ${patternId}`] : [];
    const procId = enhancement.procId;
    const proc = WEAPON_SPECIALS.find((entry) => entry.procId === procId);
    const special = procId !== void 0 && procId > 0 ? proc === void 0 ? `Proc ${procId}` : namedValueDisplayName(proc) : patternSpecial;
    return { name, special };
  }, "getItemEnhancementNames");
  var normalize2 = /* @__PURE__ */ __name((value) => value.trim().toLowerCase().replaceAll(/[‘’]/gu, "'").replaceAll(/\s+/gu, " "), "normalize");
  var matchesNamedValue = /* @__PURE__ */ __name((entry, value) => {
    const requested = normalize2(value);
    return entry.name === requested || entry.aliases?.some((alias) => alias === requested) === true;
  }, "matchesNamedValue");
  var resolveNamedValue = /* @__PURE__ */ __name((entries, value) => entries.find((entry) => matchesNamedValue(entry, value)), "resolveNamedValue");
  var resolvePatternId = /* @__PURE__ */ __name((entries, value) => resolveNamedValue(entries, value)?.patternId, "resolvePatternId");
  var inventoryHelmName = /* @__PURE__ */ __name((entry) => entry.inventoryName ?? entry.name, "inventoryHelmName");
  var resolveInventoryHelmEnhancement = /* @__PURE__ */ __name((enhancement, special) => HELM_ENHANCEMENTS.find(
    (entry) => normalize2(inventoryHelmName(entry)) === normalize2(enhancement) && matchesNamedValue(entry.special, special)
  ), "resolveInventoryHelmEnhancement");
  var enhancementSlot = /* @__PURE__ */ __name((item) => {
    if (item.weapon) return "weapon";
    if (item.cape) return "cape";
    if (item.helm) return "helm";
    if (item.classItem) return "class";
    return void 0;
  }, "enhancementSlot");
  var basicStrategy = /* @__PURE__ */ __name((enhancement, playerLevel, slot) => ({
    ok: true,
    strategy: {
      patternId: enhancement.patternId,
      procId: NO_PROC_ID,
      shopId: enhancement.shopIds[playerLevel >= BASIC_HIGH_TIER_MINIMUM_PLAYER_LEVEL ? "high" : "low"],
      slot
    }
  }), "basicStrategy");
  var resolveEnhancementStrategy = /* @__PURE__ */ __name((item, enhancement, playerLevel, special) => {
    const slot = enhancementSlot(item);
    if (slot === void 0) {
      return { ok: false, reason: "The selected item cannot be enhanced" };
    }
    const requested = normalize2(enhancement);
    const requestedSpecial = normalize2(special ?? "");
    const basicEnhancement = resolveNamedValue(
      BASIC_ENHANCEMENTS,
      requested
    );
    const forge = requested === "forge";
    if (slot === "class") {
      if (forge || requestedSpecial !== "" || basicEnhancement === void 0) {
        return {
          ok: false,
          reason: "The requested enhancement cannot be applied to a class"
        };
      }
      return basicStrategy(basicEnhancement, playerLevel, slot);
    }
    if (slot === "weapon") {
      const explicitSpecial = requestedSpecial === "" ? void 0 : resolveNamedValue(WEAPON_SPECIALS, requestedSpecial);
      if (requestedSpecial !== "" && explicitSpecial === void 0) {
        return { ok: false, reason: "Unknown weapon enhancement special" };
      }
      const shorthandSpecial = requestedSpecial === "" ? resolveNamedValue(WEAPON_SPECIALS, requested) : void 0;
      const weaponSpecial = explicitSpecial ?? (shorthandSpecial?.family === "forge" ? shorthandSpecial : void 0);
      const requestedPatternId = basicEnhancement?.patternId ?? resolvePatternId(FORGE_WEAPON_PATTERNS, requested);
      if (weaponSpecial === void 0) {
        if (basicEnhancement !== void 0) {
          return basicStrategy(basicEnhancement, playerLevel, slot);
        }
        if (requestedPatternId !== PATTERN_IDS.forge) {
          return { ok: false, reason: "Unknown weapon enhancement" };
        }
        return {
          ok: true,
          strategy: {
            map: "forge",
            patternId: PATTERN_IDS.forge,
            procId: NO_PROC_ID,
            shopId: FORGE_SHOP_IDS.weapon,
            slot
          }
        };
      }
      if (weaponSpecial.family === "awe") {
        if (basicEnhancement === void 0) {
          return {
            ok: false,
            reason: "Awe specials require a basic enhancement"
          };
        }
        const shopId = basicEnhancement.aweShopId;
        return shopId === void 0 ? { ok: false, reason: "No Awe enhancement shop matched the request" } : {
          ok: true,
          strategy: {
            map: "museum",
            patternId: basicEnhancement.patternId,
            procId: weaponSpecial.procId,
            shopId,
            slot
          }
        };
      }
      const usesForgeFamily = forge && explicitSpecial !== void 0;
      const usesStandaloneLabel = shorthandSpecial === weaponSpecial;
      if (!usesForgeFamily && !usesStandaloneLabel && requestedPatternId !== weaponSpecial.patternId) {
        return {
          ok: false,
          reason: "The enhancement and special do not match their inventory label"
        };
      }
      return {
        ok: true,
        strategy: {
          map: "forge",
          patternId: weaponSpecial.patternId,
          procId: weaponSpecial.procId,
          shopId: FORGE_SHOP_IDS.weapon,
          slot
        }
      };
    }
    if (slot === "helm") {
      if (basicEnhancement !== void 0 && requestedSpecial === "") {
        return basicStrategy(basicEnhancement, playerLevel, slot);
      }
      if (forge && requestedSpecial === "") {
        return {
          ok: true,
          strategy: {
            map: "forge",
            patternId: PATTERN_IDS.forge,
            procId: NO_PROC_ID,
            shopId: FORGE_SHOP_IDS.helm,
            slot
          }
        };
      }
      const helmEnhancement = forge ? resolveNamedValue(HELM_ENHANCEMENTS, requestedSpecial) : requestedSpecial === "" ? resolveNamedValue(HELM_ENHANCEMENTS, requested) : resolveInventoryHelmEnhancement(requested, requestedSpecial);
      if (helmEnhancement === void 0) {
        return {
          ok: false,
          reason: "Unknown Forge helm enhancement"
        };
      }
      return {
        ok: true,
        strategy: {
          map: "forge",
          patternId: helmEnhancement.patternId,
          procId: NO_PROC_ID,
          shopId: FORGE_SHOP_IDS.helm,
          slot
        }
      };
    }
    if (basicEnhancement !== void 0 && requestedSpecial === "") {
      return basicStrategy(basicEnhancement, playerLevel, slot);
    }
    const shorthandCape = requestedSpecial === "" ? resolveNamedValue(CAPE_SPECIALS, requested) : void 0;
    const patternId = forge ? requestedSpecial === "" ? PATTERN_IDS.forge : resolvePatternId(CAPE_SPECIALS, requestedSpecial) : shorthandCape?.patternId;
    if (patternId === void 0) {
      return { ok: false, reason: "Unknown Forge cape enhancement" };
    }
    return {
      ok: true,
      strategy: {
        map: "forge",
        patternId,
        procId: NO_PROC_ID,
        shopId: FORGE_SHOP_IDS.cape,
        slot
      }
    };
  }, "resolveEnhancementStrategy");
  var normalizedEquipmentSlot = /* @__PURE__ */ __name((item) => normalize2(item.equipmentSlot), "normalizedEquipmentSlot");
  var matchesSlot = /* @__PURE__ */ __name((item, slot) => {
    const actual = normalizedEquipmentSlot(item);
    switch (slot) {
      case "weapon":
        return actual === "weapon";
      case "cape":
        return actual === "ba" || actual === "cape";
      case "helm":
        return actual === "he" || actual === "helm";
      case "class":
        return actual === "ar" || actual === "class";
    }
  }, "matchesSlot");
  var matchesEnhancementShopItem = /* @__PURE__ */ __name((item, strategy) => {
    if (normalize2(item.category) !== "enhancement") return false;
    if (!matchesSlot(item, strategy.slot)) return false;
    const patternId = item.enhancement?.patternId ?? NO_PATTERN_ID;
    const procId = item.enhancement?.procId ?? NO_PROC_ID;
    return patternId === strategy.patternId && procId === strategy.procId;
  }, "matchesEnhancementShopItem");
  var matchesAppliedEnhancement = /* @__PURE__ */ __name((item, strategy) => {
    const patternId = item.enhancement?.patternId ?? NO_PATTERN_ID;
    const procId = item.enhancement?.procId ?? NO_PROC_ID;
    return patternId === strategy.patternId && procId === strategy.procId;
  }, "matchesAppliedEnhancement");

  // packages/game/src/protocol.ts
  var EntityState = { Dead: 0, Idle: 1, InCombat: 2 };

  // packages/game/src/entity.ts
  var percent = /* @__PURE__ */ __name((value, maximum) => maximum <= 0 ? 0 : value / maximum * 100, "percent");
  var LiveEntity = class extends LiveModel {
    static {
      __name(this, "LiveEntity");
    }
    #auras;
    constructor(data, auras = []) {
      super(data);
      this.#auras = auras;
    }
    get alive() {
      return this.hp > 0 && this.state !== EntityState.Dead;
    }
    get auras() {
      return [...this.#auras];
    }
    get cell() {
      return this.modelData.cell;
    }
    get dead() {
      return !this.alive;
    }
    get hp() {
      return this.modelData.hp;
    }
    get hpPercent() {
      return percent(this.hp, this.maxHp);
    }
    get idle() {
      return this.state === EntityState.Idle;
    }
    get inCombat() {
      return this.state === EntityState.InCombat;
    }
    get maxHp() {
      return this.modelData.maxHp;
    }
    get maxMp() {
      return this.modelData.maxMp;
    }
    get mp() {
      return this.modelData.mp;
    }
    get mpPercent() {
      return percent(this.mp, this.maxMp);
    }
    get state() {
      return this.modelData.state;
    }
    getAura(name, options) {
      const normalizedName = normalizeGameText(name);
      for (const aura of this.#auras) {
        if (normalizeGameText(aura.name) === normalizedName && (options?.kind === void 0 || aura.kind === options.kind)) {
          return aura;
        }
      }
      return null;
    }
    hasAura(name, options) {
      return this.getAura(name, options) !== null;
    }
    isInCell(cell) {
      return normalizeGameText(this.cell) === normalizeGameText(cell);
    }
  };

  // packages/game/src/faction.ts
  var LiveFaction = class extends LiveModel {
    static {
      __name(this, "LiveFaction");
    }
    get id() {
      return this.modelData.id;
    }
    get name() {
      return this.modelData.name;
    }
    get rank() {
      return this.modelData.rank;
    }
    get reputation() {
      return this.modelData.reputation;
    }
    toJSON() {
      return { ...this.modelData };
    }
  };

  // packages/game/src/item.ts
  var miscCategories = /* @__PURE__ */ new Set(["Item", "Note", "Quest Item", "Resource"]);
  var classRankPointThresholds = [
    900,
    3600,
    1e4,
    22500,
    44100,
    78400,
    129600,
    202500,
    302500
  ];
  var getClassRankFromPoints = /* @__PURE__ */ __name((classPoints) => {
    if (!Number.isFinite(classPoints)) return 1;
    const points = Math.max(0, Math.trunc(classPoints));
    let rank = 1;
    for (const threshold of classRankPointThresholds) {
      if (points < threshold) break;
      rank += 1;
    }
    return rank;
  }, "getClassRankFromPoints");
  var toItemSelector = /* @__PURE__ */ __name((query) => {
    if (typeof query === "number") return { itemId: query };
    if (typeof query === "string") return { name: query.trim() };
    return "name" in query ? { name: query.name.trim() } : query;
  }, "toItemSelector");
  var normalizeItemQuantity = /* @__PURE__ */ __name((value) => value === void 0 || !Number.isFinite(value) ? 1 : Math.max(1, Math.trunc(value)), "normalizeItemQuantity");
  var LiveItem = class extends LiveModel {
    static {
      __name(this, "LiveItem");
    }
    get armor() {
      return this.equipmentSlot === "co";
    }
    get banked() {
      return this.context === "bank";
    }
    get cape() {
      return this.equipmentSlot === "ba";
    }
    get category() {
      return this.modelData.category;
    }
    get charItemId() {
      return this.modelData.charItemId;
    }
    get classItem() {
      return this.category === "Class";
    }
    get classRank() {
      return this.classItem ? getClassRankFromPoints(this.quantity) : null;
    }
    get coins() {
      return this.modelData.coins;
    }
    get context() {
      return this.modelData.context;
    }
    get cost() {
      return this.modelData.cost;
    }
    get description() {
      return this.modelData.description;
    }
    get enhancement() {
      const enhancement = this.modelData.enhancement;
      if (enhancement === void 0) return void 0;
      return { ...enhancement, ...getItemEnhancementNames(enhancement) };
    }
    get equipped() {
      return this.modelData.equipped;
    }
    get equipmentSlot() {
      return this.modelData.equipmentSlot;
    }
    get file() {
      return this.modelData.file;
    }
    get helm() {
      return this.equipmentSlot === "he";
    }
    get houseItem() {
      return this.modelData.houseItem;
    }
    get itemId() {
      return this.modelData.itemId;
    }
    get link() {
      return this.modelData.link;
    }
    get maxStack() {
      return this.modelData.maxStack;
    }
    get memberOnly() {
      return this.modelData.memberOnly;
    }
    get meta() {
      return this.modelData.meta;
    }
    get name() {
      return this.modelData.name;
    }
    get pet() {
      return this.equipmentSlot === "pe";
    }
    get pool() {
      if (this.classItem) return "class";
      if (this.houseItem || this.category === "Guild") return "house";
      return miscCategories.has(this.category) && !(this.category === "Item" && /^\s*\d+\s*$/.test(this.meta)) ? "misc" : "bag";
    }
    get quantity() {
      return this.modelData.quantity;
    }
    get requirements() {
      return this.modelData.requirements ?? [];
    }
    get shopItemId() {
      return this.modelData.shopItemId;
    }
    get temporaryItem() {
      return this.modelData.temporaryItem;
    }
    get weapon() {
      return this.equipmentSlot === "Weapon";
    }
    get wearable() {
      return this.modelData.wearable ?? false;
    }
    get worn() {
      return this.modelData.worn ?? false;
    }
    matches(selector) {
      if (typeof selector === "number") return this.itemId === selector;
      if (typeof selector === "string") {
        return normalizeGameText(this.name) === normalizeGameText(selector);
      }
      if ("itemId" in selector) return this.itemId === selector.itemId;
      if ("name" in selector)
        return normalizeGameText(this.name) === normalizeGameText(selector.name);
      if ("shopItemId" in selector)
        return this.shopItemId === selector.shopItemId;
      return false;
    }
    toJSON() {
      const { enhancement, ...data } = this.modelData;
      const { name, special, ...enhancementData } = this.enhancement ?? {};
      return {
        ...data,
        armor: this.armor,
        banked: this.banked,
        cape: this.cape,
        classItem: this.classItem,
        classRank: this.classRank,
        ...enhancement === void 0 ? {} : {
          enhancement: {
            ...enhancementData,
            ...name === void 0 ? {} : { name },
            ...special === void 0 ? {} : { special }
          }
        },
        helm: this.helm,
        pet: this.pet,
        pool: this.pool,
        ...this.modelData.requirements === void 0 ? {} : {
          requirements: this.requirements.map((requirement) => ({
            ...requirement
          }))
        },
        wearable: this.wearable,
        weapon: this.weapon,
        worn: this.worn
      };
    }
  };
  var toLiveItem = /* @__PURE__ */ __name((snapshot) => {
    const {
      armor,
      banked,
      cape,
      classItem,
      classRank,
      enhancement,
      helm,
      pet,
      pool,
      weapon,
      ...data
    } = snapshot;
    if (enhancement === void 0) return new LiveItem(data);
    const { name, special, ...enhancementData } = enhancement;
    return new LiveItem({ ...data, enhancement: enhancementData });
  }, "toLiveItem");

  // packages/game/src/monster.ts
  var monsterMapIdToken = /^id[.:'-]([1-9]\d*)$/iu;
  var parseMonsterMapId = /* @__PURE__ */ __name((value) => {
    const match6 = value.trim().match(monsterMapIdToken);
    return match6?.[1] === void 0 ? void 0 : Number(match6[1]);
  }, "parseMonsterMapId");
  var toMonsterSelector = /* @__PURE__ */ __name((query) => {
    if (typeof query === "number") return { monsterMapId: query };
    if (typeof query === "object") {
      return "monsterMapId" in query ? query : { name: query.name.trim() };
    }
    const monsterMapId = parseMonsterMapId(query);
    return monsterMapId === void 0 ? { name: query.trim() } : { monsterMapId };
  }, "toMonsterSelector");
  var orderMonstersByPriority = /* @__PURE__ */ __name((monsters, priorities) => {
    const ordered = [];
    const selected = /* @__PURE__ */ new Set();
    for (const priority of priorities) {
      for (const monster of monsters) {
        if (!selected.has(monster.monsterMapId) && monster.matches(priority)) {
          ordered.push(monster);
          selected.add(monster.monsterMapId);
        }
      }
    }
    return ordered;
  }, "orderMonstersByPriority");
  var LiveMonster = class extends LiveEntity {
    static {
      __name(this, "LiveMonster");
    }
    #drops;
    constructor(data, auras = [], drops = []) {
      super(data, auras);
      this.#drops = drops;
    }
    get aggressive() {
      return this.modelData.aggressive;
    }
    get drops() {
      return [...this.#drops];
    }
    get level() {
      return this.modelData.level;
    }
    get monsterId() {
      return this.modelData.monsterId;
    }
    get monsterMapId() {
      return this.modelData.monsterMapId;
    }
    get name() {
      return this.modelData.name;
    }
    get race() {
      return this.modelData.race;
    }
    matches(selector) {
      if (typeof selector === "number") return this.monsterMapId === selector;
      if (typeof selector === "string") {
        const monsterMapId = parseMonsterMapId(selector);
        if (monsterMapId !== void 0) {
          return this.monsterMapId === monsterMapId;
        }
      } else if ("monsterMapId" in selector) {
        return this.monsterMapId === selector.monsterMapId;
      }
      const name = normalizeGameText(
        typeof selector === "string" ? selector : selector.name
      );
      return name === "*" || name !== "" && normalizeGameText(this.name) === name;
    }
    toJSON() {
      return {
        ...this.modelData,
        alive: this.alive,
        auras: this.auras.map((aura) => aura.toJSON()),
        dead: this.dead,
        drops: this.drops.map((drop) => ({
          ...drop,
          item: { ...drop.item },
          questObjectives: [...drop.questObjectives],
          requiredQuestIds: [...drop.requiredQuestIds],
          requiredQuests: [...drop.requiredQuests]
        })),
        hpPercent: this.hpPercent,
        idle: this.idle,
        inCombat: this.inCombat,
        mpPercent: this.mpPercent
      };
    }
  };
  var toLiveMonster = /* @__PURE__ */ __name((snapshot) => {
    const {
      alive,
      auras,
      dead,
      drops,
      hpPercent,
      idle,
      inCombat,
      mpPercent,
      ...data
    } = snapshot;
    return new LiveMonster(data, auras.map(toLiveAura), drops);
  }, "toLiveMonster");

  // packages/game/src/outfit.ts
  var LiveOutfit = class extends LiveModel {
    static {
      __name(this, "LiveOutfit");
    }
    get colors() {
      return this.modelData.colors;
    }
    get equipment() {
      return this.modelData.equipment;
    }
    get name() {
      return this.modelData.name;
    }
    toJSON() {
      return {
        ...this.modelData,
        colors: { ...this.colors },
        equipment: { ...this.equipment }
      };
    }
  };

  // packages/game/src/player.ts
  var toPlayerSelector = /* @__PURE__ */ __name((query) => ({
    username: typeof query === "string" ? query.trim() : query.username.trim()
  }), "toPlayerSelector");
  var LivePlayer = class extends LiveEntity {
    static {
      __name(this, "LivePlayer");
    }
    get afk() {
      return this.modelData.afk;
    }
    get entityId() {
      return this.modelData.entityId;
    }
    get entityType() {
      return this.modelData.entityType;
    }
    get level() {
      return this.modelData.level;
    }
    get pad() {
      return this.modelData.pad;
    }
    get position() {
      return this.modelData.position;
    }
    get username() {
      return this.modelData.username;
    }
    toJSON() {
      return {
        ...this.modelData,
        alive: this.alive,
        auras: this.auras.map((aura) => aura.toJSON()),
        dead: this.dead,
        hpPercent: this.hpPercent,
        idle: this.idle,
        inCombat: this.inCombat,
        mpPercent: this.mpPercent,
        position: { ...this.position }
      };
    }
  };
  var toLivePlayer = /* @__PURE__ */ __name((snapshot) => {
    const {
      alive,
      auras,
      dead,
      hpPercent,
      idle,
      inCombat,
      mpPercent,
      position,
      ...data
    } = snapshot;
    return new LivePlayer(
      { ...data, position: { ...position } },
      auras.map(toLiveAura)
    );
  }, "toLivePlayer");

  // packages/game/src/quest.ts
  var LiveQuest = class extends LiveModel {
    static {
      __name(this, "LiveQuest");
    }
    /**
     * How often the quest can be completed.
     */
    get cadence() {
      return this.modelData.cadence;
    }
    get id() {
      return this.modelData.id;
    }
    get name() {
      return this.modelData.name;
    }
    get once() {
      return this.modelData.once;
    }
    get requirements() {
      return this.modelData.requirements;
    }
    get rewards() {
      return this.modelData.rewards;
    }
    toJSON() {
      return {
        ...this.modelData,
        requirements: this.requirements.map((item) => ({ ...item })),
        rewards: this.rewards.map((item) => ({ ...item }))
      };
    }
  };
  var toLiveQuest = /* @__PURE__ */ __name((snapshot) => new LiveQuest({ ...snapshot }), "toLiveQuest");

  // packages/game/src/server.ts
  var LiveServer = class extends LiveModel {
    static {
      __name(this, "LiveServer");
    }
    get chat() {
      return this.modelData.chat;
    }
    get count() {
      return this.modelData.count;
    }
    get full() {
      return this.max > 0 && this.count >= this.max;
    }
    get language() {
      return this.modelData.language;
    }
    get max() {
      return this.modelData.max;
    }
    get memberOnly() {
      return this.modelData.memberOnly;
    }
    get name() {
      return this.modelData.name;
    }
    get online() {
      return this.modelData.online;
    }
    toJSON() {
      return { ...this.modelData, full: this.full };
    }
  };
  var toLiveServer = /* @__PURE__ */ __name((snapshot) => {
    const { full, ...data } = snapshot;
    return new LiveServer(data);
  }, "toLiveServer");

  // packages/game/src/shop.ts
  var LiveShop = class extends LiveModel {
    static {
      __name(this, "LiveShop");
    }
    get house() {
      return this.modelData.house;
    }
    get id() {
      return this.modelData.id;
    }
    get items() {
      return this.modelData.items;
    }
    get limited() {
      return this.modelData.limited;
    }
    get merge() {
      return this.modelData.merge;
    }
    get name() {
      return this.modelData.name;
    }
    toJSON() {
      return {
        ...this.modelData,
        items: this.items.map((item) => item.toJSON())
      };
    }
  };
  var toLiveShop = /* @__PURE__ */ __name((snapshot) => new LiveShop({ ...snapshot, items: snapshot.items.map(toLiveItem) }), "toLiveShop");

  // packages/runtime/src/game/flash/state/HostState.ts
  var HostState = class extends Service()(
    "lucent/renderer/flash/HostState"
  ) {
    static {
      __name(this, "HostState");
    }
  };
  var layer3 = effect(
    HostState,
    gen2(function* () {
      const host4 = __host;
      const diagnosticSink = yield* DiagnosticSink;
      const read = /* @__PURE__ */ __name((query) => sync2(
        () => JSON.parse(host4.read(JSON.stringify(query)))
      ), "read");
      const command = /* @__PURE__ */ __name((command2) => sync2(() => {
        try {
          return some2(
            JSON.parse(host4.command(JSON.stringify(command2)))
          );
        } catch (cause) {
          diagnosticSink.report(
            "invoke",
            command2.kind,
            cause,
            command2.kind === "login" ? void 0 : [command2]
          );
          return none2();
        }
      }), "command");
      return { command, read };
    })
  );
  var isFiniteNumber = /* @__PURE__ */ __name((value) => typeof value === "number" && Number.isFinite(value), "isFiniteNumber");
  var isSelectable = /* @__PURE__ */ __name((selector) => {
    if (typeof selector === "string") return true;
    if (typeof selector !== "object" || selector === null) {
      return isFiniteNumber(selector);
    }
    const fields = selector;
    return isFiniteNumber(fields["shopItemId"]) || isFiniteNumber(fields["itemId"]) || isFiniteNumber(fields["monsterMapId"]) || typeof fields["name"] === "string";
  }, "isSelectable");
  var readItem = /* @__PURE__ */ __name((host4, container2, selector) => isSelectable(selector) ? host4.read({ kind: "item", container: container2, selector }).pipe(map5((item) => item === null ? null : toLiveItem(item))) : succeed7(null), "readItem");
  var readItems = /* @__PURE__ */ __name((host4, container2) => host4.read({ kind: "items", container: container2 }).pipe(map5((items) => items.map(toLiveItem))), "readItems");
  var itemQuantity = /* @__PURE__ */ __name((host4, container2, selector) => readItem(host4, container2, selector).pipe(
    map5((item) => item?.quantity ?? 0)
  ), "itemQuantity");
  var isHydrated = /* @__PURE__ */ __name((host4, container2) => host4.read({ kind: "itemHydration", container: container2 }).pipe(map5((version) => version > 0)), "isHydrated");

  // packages/runtime/src/game/flash/api/Auth.ts
  var Credentials = Struct({
    password: String4,
    username: String4
  });
  var ConnectResult = Union2([
    Struct({
      message: String4,
      ok: Literal2(true),
      serverName: optionalKey2(String4),
      status: Literal2("selected")
    }),
    Struct({
      message: String4,
      ok: Literal2(false),
      reason: optionalKey2(String4),
      serverName: optionalKey2(String4),
      status: Literals(["blocked", "not-found", "not-ready"])
    })
  ]);
  var decodeCredentials = decodeUnknownOption2(Credentials);
  var temporaryKickFallbackMs = 6e4;
  var temporaryKickMaximumMs = 7e4;
  var makeAuth = /* @__PURE__ */ __name((bridge, host4, wait) => {
    const cached2 = host4.read({ kind: "auth" });
    const loggedIn = host4.command({ kind: "refreshLoggedIn" }).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => cached2.pipe(map5((auth) => auth.loggedIn)), "onNone"),
          onSome: succeed7
        })
      )
    );
    const currentCredentials = host4.command({ kind: "refreshCredentials" }).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => cached2, "onNone"),
          onSome: succeed7
        })
      )
    );
    const temporarilyKicked = bridge.invoke("auth.isTemporarilyKicked", void 0, Boolean3).pipe(map5(getOrElse(() => false)));
    const getTemporaryKickRemainingMs = /* @__PURE__ */ __name(() => bridge.invokeJson("flash.getGameObject", ["mcLogin.warning.n"], Number6).pipe(
      map5(
        match({
          onNone: /* @__PURE__ */ __name(() => void 0, "onNone"),
          onSome: /* @__PURE__ */ __name((seconds2) => Number.isFinite(seconds2) && seconds2 > 0 ? Math.min(temporaryKickMaximumMs, Math.ceil(seconds2) * 1e3) : void 0, "onSome")
        })
      )
    ), "getTemporaryKickRemainingMs");
    const waitForTemporaryKickClear = gen2(function* () {
      if (!(yield* temporarilyKicked)) return true;
      const remainingMs = (yield* getTemporaryKickRemainingMs()) ?? temporaryKickFallbackMs;
      const initialSleepMs = Math.min(
        temporaryKickMaximumMs,
        remainingMs + 1e3
      );
      yield* sleep2(`${initialSleepMs} millis`);
      if (!(yield* temporarilyKicked)) return true;
      const finalWaitMs = Math.max(0, temporaryKickMaximumMs - initialSleepMs);
      if (finalWaitMs === 0) return false;
      return yield* wait.until(
        temporarilyKicked.pipe(map5((kicked) => !kicked)),
        {
          interval: "5 seconds",
          timeout: `${finalWaitMs} millis`
        }
      );
    });
    const connectTo = /* @__PURE__ */ __name((server) => {
      const requested = server.trim();
      if (requested === "") {
        return succeed7({
          message: "server is required",
          retryable: false,
          status: "not-found"
        });
      }
      const select = bridge.invoke("auth.connectTo", [requested], ConnectResult).pipe(
        map5(
          match({
            onNone: /* @__PURE__ */ __name(() => ({
              message: "server selection is not ready",
              retryable: true,
              serverName: requested,
              status: "not-ready"
            }), "onNone"),
            onSome: /* @__PURE__ */ __name((result2) => result2.ok ? {
              message: result2.message,
              retryable: false,
              serverName: result2.serverName ?? requested,
              status: "connected"
            } : {
              message: result2.message,
              retryable: result2.status === "not-ready" || result2.reason === "full",
              serverName: result2.serverName ?? requested,
              status: result2.reason === "full" ? "full" : result2.status
            }, "onSome")
          })
        )
      );
      return gen2(function* () {
        const initial = yield* select;
        if (initial.status !== "not-ready") return initial;
        yield* sleep2("250 millis");
        const settled = yield* wait.untilSome(
          select.pipe(
            map5(
              (outcome2) => outcome2.status === "not-ready" ? none2() : some2(outcome2)
            )
          ),
          { interval: "500 millis", timeout: "5 seconds" }
        );
        return settled ?? initial;
      });
    }, "connectTo");
    const getPassword = /* @__PURE__ */ __name(() => currentCredentials.pipe(map5((current) => current.password)), "getPassword");
    const getServers = /* @__PURE__ */ __name(() => host4.command({ kind: "refreshServers" }).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => cached2.pipe(map5((auth) => auth.servers)), "onNone"),
          onSome: succeed7
        })
      ),
      map5((servers) => servers.map(toLiveServer))
    ), "getServers");
    const getUsername = /* @__PURE__ */ __name(() => currentCredentials.pipe(map5((current) => current.username)), "getUsername");
    const isServerSelectReady = /* @__PURE__ */ __name(() => bridge.invoke("flash.isNull", ["mcLogin.sl.iList"], Boolean3).pipe(map5((value) => !getOrElse(value, () => true))), "isServerSelectReady");
    const isLoggedIn = /* @__PURE__ */ __name(() => loggedIn, "isLoggedIn");
    const isTemporarilyKicked = /* @__PURE__ */ __name(() => temporarilyKicked, "isTemporarilyKicked");
    const login = /* @__PURE__ */ __name((username, password) => {
      const credentials = decodeCredentials({
        password,
        username: username.trim()
      });
      if (isNone2(credentials) || credentials.value.username === "" || credentials.value.password === "") {
        return succeed7(false);
      }
      return waitForTemporaryKickClear.pipe(
        flatMap3(
          (ready) => ready ? host4.command({
            kind: "login",
            username: credentials.value.username,
            password: credentials.value.password
          }).pipe(map5(getOrElse(() => false))) : succeed7(false)
        )
      );
    }, "login");
    const logout = /* @__PURE__ */ __name(() => host4.command({ kind: "logout" }), "logout");
    return {
      connectTo,
      getPassword,
      getServers,
      getUsername,
      isLoggedIn,
      isServerSelectReady,
      isTemporarilyKicked,
      login,
      logout
    };
  }, "makeAuth");

  // packages/runtime/src/game/flash/api/internal/CombatControl.ts
  var EntityStatePayload = WireInt.check(
    isBetween2({
      minimum: EntityState.Dead,
      maximum: EntityState.InCombat
    })
  );
  var CombatTargetPayload = NullOr(
    Union2([
      Struct({
        cell: String4,
        hp: WireInt,
        level: WireInt,
        maxHp: WireInt,
        monsterId: WireInt,
        monsterMapId: WireInt,
        name: String4,
        race: String4,
        state: EntityStatePayload,
        type: Literal2("monster")
      }),
      Struct({
        afk: WireBoolean,
        cell: String4,
        entityId: WireInt,
        entityType: String4,
        hp: WireInt,
        level: WireInt,
        maxHp: WireInt,
        maxMp: WireInt,
        mp: WireInt,
        name: String4,
        pad: String4,
        sp: WireInt,
        state: EntityStatePayload,
        type: Literal2("player"),
        username: String4
      })
    ])
  );
  var readCombatTarget = /* @__PURE__ */ __name((bridge) => bridge.invoke("combat.getTarget", void 0, CombatTargetPayload).pipe(map5(getOrNull)), "readCombatTarget");
  var stopCombat = /* @__PURE__ */ __name((bridge) => all2(
    [
      bridge.invoke("combat.cancelAutoAttack", void 0, Void2),
      bridge.invoke("combat.cancelTarget", void 0, Void2)
    ],
    { discard: true }
  ).pipe(asVoid2), "stopCombat");

  // packages/runtime/src/game/flash/api/internal/AntiCounter.ts
  var SIGNAL_TIMEOUT_MS = 7e3;
  var resetConnectionStatuses = /* @__PURE__ */ new Set([
    "OnConnection",
    "OnConnectionFailed",
    "OnConnectionLost"
  ]);
  var makeAntiCounter = /* @__PURE__ */ __name((bridge, isEnabled) => {
    const tracked = /* @__PURE__ */ new Map();
    const isActive = fn2("AntiCounter.isActive")(function* (monsterMapId) {
      const current = tracked.get(monsterMapId);
      if (current === void 0) return false;
      if (current.recoverAtMs > (yield* currentTimeMillis2)) return true;
      tracked.delete(monsterMapId);
      return false;
    });
    const clear3 = /* @__PURE__ */ __name((monsterMapId) => sync2(() => {
      if (monsterMapId === void 0) {
        tracked.clear();
      } else {
        tracked.delete(monsterMapId);
      }
    }), "clear");
    const handleStart = fn2("AntiCounter.handleStart")(function* (event) {
      const now = yield* currentTimeMillis2;
      const previous = tracked.get(event.monsterMapId);
      const wasActive = previous !== void 0 && previous.recoverAtMs > now;
      const durationMs = event.durationMs;
      const recoverAtMs = now + (durationMs !== void 0 && Number.isFinite(durationMs) && durationMs > 0 ? durationMs : 0) + SIGNAL_TIMEOUT_MS;
      tracked.set(event.monsterMapId, {
        cancellationIssued: wasActive ? previous.cancellationIssued : false,
        // A warning for the next cycle must extend protection even if the previous
        // aura removal was missed, without shortening an existing counter window.
        recoverAtMs: wasActive ? Math.max(previous.recoverAtMs, recoverAtMs) : recoverAtMs
      });
      if (wasActive && previous.cancellationIssued || !(yield* isEnabled())) {
        return;
      }
      const target = yield* readCombatTarget(bridge);
      if (target?.type !== "monster" || target.monsterMapId !== event.monsterMapId) {
        return;
      }
      yield* stopCombat(bridge);
      const current = tracked.get(event.monsterMapId);
      if (current !== void 0) {
        tracked.set(event.monsterMapId, {
          ...current,
          cancellationIssued: true
        });
      }
    });
    const handleEvent = fn2("AntiCounter.handleEvent")(function* (event) {
      switch (event.type) {
        case "counter-attack-start":
          yield* handleStart(event);
          return;
        case "counter-attack-end":
        case "monster-death":
        case "monster-respawn":
          yield* clear3(event.monsterMapId);
          return;
        case "join-map":
          yield* clear3();
          return;
        case "connection":
          if (resetConnectionStatuses.has(event.status)) yield* clear3();
          return;
        default:
          return;
      }
    });
    return {
      handleEvent,
      isActive
    };
  }, "makeAntiCounter");

  // packages/runtime/src/game/flash/api/Bank.ts
  var TransferResponse = Struct({
    ItemID: PositiveWireInt,
    bSuccess: optionalKey2(WireBoolean)
  });
  var SwapResponse = Struct({
    bankItemID: PositiveWireInt,
    invItemID: PositiveWireInt
  });
  var decodeTransferResponse = decodeUnknownOption2(TransferResponse);
  var decodeSwapResponse = decodeUnknownOption2(SwapResponse);
  var houseCanAccept = /* @__PURE__ */ __name((itemId, house) => house.get(itemId).pipe(
    flatMap3(
      (current) => current !== null ? succeed7(true) : house.getAvailableSlots().pipe(map5((available) => available > 0))
    )
  ), "houseCanAccept");
  var makeBank = fnUntraced2(function* (bridge, host4, auth, inventory, house, wait) {
    const loads = yield* make15(1);
    const opens = yield* make15(1);
    const isOpen = /* @__PURE__ */ __name(() => bridge.invoke("bank.isOpen", void 0, Boolean3).pipe(map5(getOrElse(() => false))), "isOpen");
    const getAll = /* @__PURE__ */ __name(() => isHydrated(host4, "bank").pipe(
      flatMap3(
        (hydrated) => hydrated ? readItems(host4, "bank") : succeed7([])
      )
    ), "getAll");
    const get5 = /* @__PURE__ */ __name((selector) => isHydrated(host4, "bank").pipe(
      flatMap3(
        (hydrated) => hydrated ? readItem(host4, "bank", selector) : succeed7(null)
      )
    ), "get");
    const getSlots = /* @__PURE__ */ __name(() => bridge.invoke("bank.getSlots", void 0, WireInt).pipe(map5(getOrElse(() => 0))), "getSlots");
    const getUsedSlots = /* @__PURE__ */ __name(() => bridge.invoke("bank.getUsedSlots", void 0, WireInt).pipe(map5(getOrElse(() => 0))), "getUsedSlots");
    const isLoaded = /* @__PURE__ */ __name(() => bridge.invoke("bank.isLoaded", void 0, Boolean3).pipe(map5(getOrElse(() => false))), "isLoaded");
    const load = /* @__PURE__ */ __name((force = false) => {
      return loads.withPermits(1)(
        gen2(function* () {
          if (!(yield* auth.isLoggedIn())) return false;
          const version = yield* host4.read({
            kind: "itemHydration",
            container: "bank"
          });
          if (version > 0 && !force) return true;
          const requested = yield* bridge.invoke(
            "bank.loadItems",
            [force || version === 0],
            Void2
          );
          if (isNone2(requested)) return false;
          const loaded = yield* wait.until(isLoaded(), {
            interval: "100 millis",
            timeout: "10 seconds"
          });
          if (!loaded) return false;
          const refreshed = yield* host4.command({
            kind: "refreshContainer",
            container: "bank"
          });
          return isSome2(refreshed);
        })
      );
    }, "load");
    const open = /* @__PURE__ */ __name((options = {}) => opens.withPermits(1)(
      gen2(function* () {
        if (!(yield* load(options.force ?? false))) return false;
        if (yield* isOpen()) return true;
        if (isNone2(
          yield* bridge.invoke("bank.open", void 0, Void2)
        )) {
          return false;
        }
        return yield* wait.until(isOpen(), { timeout: "3 seconds" });
      })
    ), "open");
    const contains = /* @__PURE__ */ __name((selector, quantity) => get5(selector).pipe(
      map5(
        (item) => item !== null && item.quantity >= normalizeItemQuantity(quantity)
      )
    ), "contains");
    const bankCanAccept = /* @__PURE__ */ __name((item) => item.coins ? succeed7(true) : getAvailableSlots().pipe(map5((slots) => slots > 0)), "bankCanAccept");
    const deposit = /* @__PURE__ */ __name((selector) => {
      return gen2(function* () {
        if (!(yield* open())) return false;
        const inventoryItem = yield* inventory.get(selector);
        if (inventoryItem === null || inventoryItem.category === "Class" || inventoryItem.equipped || !(yield* bankCanAccept(inventoryItem))) {
          return false;
        }
        let response;
        const packet = yield* wait.forPacket(
          {
            command: "bankFromInv",
            direction: "extension",
            predicate: /* @__PURE__ */ __name((candidate) => {
              const decoded = decodeTransferResponse(packetData(candidate));
              if (isNone2(decoded) || decoded.value.ItemID !== inventoryItem.itemId) {
                return false;
              }
              response = decoded.value;
              return true;
            }, "predicate"),
            encoding: "json"
          },
          {
            timeout: "5 seconds",
            trigger: bridge.invoke("bank.deposit", [toItemSelector(selector)], Boolean3).pipe(map5(getOrElse(() => false)))
          }
        );
        if (packet === null || response?.bSuccess === false) return false;
        const [projectedInventoryItem, projectedBankItem] = yield* all2([
          readItem(host4, "inventory", inventoryItem.itemId),
          readItem(host4, "bank", inventoryItem.itemId)
        ]);
        return projectedInventoryItem === null && projectedBankItem !== null;
      });
    }, "deposit");
    const withdraw = /* @__PURE__ */ __name((selector) => {
      return gen2(function* () {
        if (!(yield* load())) return false;
        const bankItem = yield* get5(selector);
        if (bankItem === null) return false;
        if (!(yield* open())) return false;
        const canAccept = bankItem.houseItem ? houseCanAccept(bankItem.itemId, house) : inventory.canAccept(bankItem);
        if (!(yield* canAccept)) {
          return false;
        }
        let response;
        const packet = yield* wait.forPacket(
          {
            command: "bankToInv",
            direction: "extension",
            predicate: /* @__PURE__ */ __name((candidate) => {
              const decoded = decodeTransferResponse(packetData(candidate));
              if (isNone2(decoded) || decoded.value.ItemID !== bankItem.itemId) {
                return false;
              }
              response = decoded.value;
              return true;
            }, "predicate"),
            encoding: "json"
          },
          {
            timeout: "5 seconds",
            trigger: bridge.invoke("bank.withdraw", [toItemSelector(selector)], Boolean3).pipe(map5(getOrElse(() => false)))
          }
        );
        if (packet === null || response?.bSuccess === false) return false;
        const [bank, inventoryItem, houseItem] = yield* all2([
          readItem(host4, "bank", bankItem.itemId),
          readItem(host4, "inventory", bankItem.itemId),
          readItem(host4, "house", bankItem.itemId)
        ]);
        return bank === null && (inventoryItem !== null || houseItem !== null);
      });
    }, "withdraw");
    const swap = /* @__PURE__ */ __name((inventorySelector, bankSelector) => {
      return gen2(function* () {
        if (!(yield* open())) return false;
        const inventoryItem = yield* inventory.get(inventorySelector);
        const bankItem = yield* get5(bankSelector);
        if (inventoryItem === null || bankItem === null || inventoryItem.category === "Class" || inventoryItem.equipped) {
          return false;
        }
        if (!inventoryItem.coins && bankItem.coins && !(yield* bankCanAccept(inventoryItem))) {
          return false;
        }
        if (!(yield* inventory.canAccept(bankItem, inventoryItem))) return false;
        const packet = yield* wait.forPacket(
          {
            command: "bankSwapInv",
            direction: "extension",
            predicate: /* @__PURE__ */ __name((candidate) => {
              const decoded = decodeSwapResponse(packetData(candidate));
              return isSome2(decoded) && decoded.value.invItemID === inventoryItem.itemId && decoded.value.bankItemID === bankItem.itemId;
            }, "predicate"),
            encoding: "json"
          },
          {
            timeout: "5 seconds",
            trigger: bridge.invoke(
              "bank.swap",
              [
                toItemSelector(inventoryItem.itemId),
                toItemSelector(bankItem.itemId)
              ],
              Boolean3
            ).pipe(map5(getOrElse(() => false)))
          }
        );
        if (packet === null) return false;
        const [newBankItem, newInventoryItem, newHouseItem] = yield* all2([
          readItem(host4, "bank", inventoryItem.itemId),
          readItem(host4, "inventory", bankItem.itemId),
          readItem(host4, "house", bankItem.itemId)
        ]);
        return newBankItem !== null && (newInventoryItem !== null || newHouseItem !== null);
      });
    }, "swap");
    const depositBatch = /* @__PURE__ */ __name((selectors) => forEach2(selectors, deposit, { concurrency: 1 }), "depositBatch");
    const withdrawBatch = /* @__PURE__ */ __name((selectors) => forEach2(selectors, withdraw, { concurrency: 1 }), "withdrawBatch");
    const getAvailableSlots = /* @__PURE__ */ __name(() => zipWith2(
      getSlots(),
      getUsedSlots(),
      (slots, used) => Math.max(0, slots - used)
    ), "getAvailableSlots");
    return {
      contains,
      deposit,
      depositBatch,
      get: get5,
      getAll,
      getAvailableSlots,
      getSlots,
      getUsedSlots,
      isOpen,
      load,
      open,
      swap,
      withdraw,
      withdrawBatch
    };
  });

  // packages/runtime/src/game/flash/contract/payload/Combat.ts
  var ActionDetailsPayload = Struct({
    actID: optionalKey2(NonNegativeWireInt),
    cInf: optionalKey2(String4),
    tInf: optionalKey2(String4)
  });
  var SingleActionAcknowledgementPayload = Struct({
    actionResult: optionalKey2(NullOr(ActionDetailsPayload)),
    actID: optionalKey2(NonNegativeWireInt),
    iRes: WireInt
  });
  var AppliedActionPayload = Struct({
    tInf: String4
  });
  var MultiActionAcknowledgementPayload = Struct({
    a: optionalKey2(NullOr(ArraySchema(Unknown2))),
    actID: NonNegativeWireInt,
    cInf: String4,
    iRes: WireInt
  });
  var CombatActionAcknowledgementsPayload = Struct({
    sara: optionalKey2(Unknown2),
    sarsa: optionalKey2(Unknown2)
  });
  var decodeActionAcknowledgementsPayload = decodeUnknownOption2(
    CombatActionAcknowledgementsPayload
  );
  var decodeSingleActionAcknowledgement = decodeUnknownOption2(
    SingleActionAcknowledgementPayload
  );
  var decodeMultiActionAcknowledgement = decodeUnknownOption2(
    MultiActionAcknowledgementPayload
  );
  var decodeAppliedAction = decodeUnknownOption2(AppliedActionPayload);
  var parseCombatEntityReferences = /* @__PURE__ */ __name((value) => value.split(",").flatMap((token) => {
    const entity = token.slice(token.lastIndexOf(">") + 1).trim();
    const [type, rawId] = entity.split(":");
    const id = Number(rawId);
    if (type !== "m" && type !== "p" || !Number.isInteger(id) || id <= 0) {
      return [];
    }
    return [
      {
        id,
        type: type === "m" ? "monster" : "player"
      }
    ];
  }), "parseCombatEntityReferences");
  var outcome = /* @__PURE__ */ __name((value) => value === 0 || value === 1 ? value : void 0, "outcome");
  var singleActionAcknowledgement = /* @__PURE__ */ __name((value) => {
    const decoded = decodeSingleActionAcknowledgement(value);
    if (isNone2(decoded)) return void 0;
    const result2 = decoded.value.actionResult;
    const acknowledgedOutcome = outcome(decoded.value.iRes);
    if (result2 == null || acknowledgedOutcome === void 0) return void 0;
    const actionId = acknowledgedOutcome === 1 ? result2.actID : decoded.value.actID;
    const source = parseCombatEntityReferences(result2.cInf ?? "")[0];
    if (actionId === void 0 || source === void 0) return void 0;
    return {
      actionId,
      outcome: acknowledgedOutcome,
      source,
      targets: acknowledgedOutcome === 1 && result2.tInf !== void 0 ? parseCombatEntityReferences(result2.tInf) : []
    };
  }, "singleActionAcknowledgement");
  var multiActionAcknowledgement = /* @__PURE__ */ __name((value) => {
    const decoded = decodeMultiActionAcknowledgement(value);
    if (isNone2(decoded)) return void 0;
    const acknowledgedOutcome = outcome(decoded.value.iRes);
    const source = parseCombatEntityReferences(decoded.value.cInf)[0];
    if (acknowledgedOutcome === void 0 || source === void 0) {
      return void 0;
    }
    return {
      actionId: decoded.value.actID,
      outcome: acknowledgedOutcome,
      source,
      targets: acknowledgedOutcome === 0 ? [] : (decoded.value.a ?? []).flatMap((value2) => {
        const applied = decodeAppliedAction(value2);
        return isNone2(applied) ? [] : parseCombatEntityReferences(applied.value.tInf);
      })
    };
  }, "multiActionAcknowledgement");
  var decodeCombatActionAcknowledgements = /* @__PURE__ */ __name((value) => {
    const decoded = decodeActionAcknowledgementsPayload(value);
    if (isNone2(decoded)) return [];
    const acknowledgements = [];
    const append3 = /* @__PURE__ */ __name((values, decode3) => {
      if (!Array.isArray(values)) return;
      for (const entry of values) {
        const acknowledgement = decode3(entry);
        if (acknowledgement !== void 0) acknowledgements.push(acknowledgement);
      }
    }, "append");
    append3(decoded.value.sara, singleActionAcknowledgement);
    append3(decoded.value.sarsa, multiActionAcknowledgement);
    return acknowledgements;
  }, "decodeCombatActionAcknowledgements");

  // packages/runtime/src/game/flash/api/internal/CombatProfileClass.ts
  var classList = new Intl.ListFormat("en", { type: "disjunction" });
  var sameClass = /* @__PURE__ */ __name((left, right) => left.localeCompare(right, void 0, { sensitivity: "accent" }) === 0, "sameClass");
  var getCombatProfileClassWarning = fn2(
    "getCombatProfileClassWarning"
  )(function* (deps, profile) {
    const classNames = profile.classNames ?? [];
    if (classNames.length === 0) {
      return void 0;
    }
    const matches = /* @__PURE__ */ __name((name) => classNames.some((expected) => sameClass(expected, name)), "matches");
    const itemName = (yield* deps.inventory.getAll()).find(
      (item) => item.classItem && item.equipped
    )?.name ?? "";
    if (matches(itemName)) {
      return void 0;
    }
    const className = yield* deps.player.getClassName();
    if (matches(className)) {
      return void 0;
    }
    const equipped = itemName || className;
    return equipped === "" ? void 0 : `This profile is for ${classList.format(classNames)}, but ${equipped} is equipped.`;
  });

  // packages/runtime/src/game/flash/api/Combat.ts
  var CONSUMABLE_CAST_CONFIRMATION_TIMEOUT_MS = 5e3;
  var ConsumableCastDispatchDeadline = Reference("lucent/game/combat/ConsumableCastDispatchDeadline", {
    defaultValue: /* @__PURE__ */ __name(() => void 0, "defaultValue")
  });
  var Consumable = NullOr(
    Struct({
      itemId: optionalKey2(WireInt),
      ItemID: optionalKey2(WireInt),
      ready: optionalKey2(Boolean3)
    })
  );
  var ConsumableCastDispatch = NullOr(
    Struct({
      actionId: NonNegativeWireInt,
      itemId: PositiveWireInt,
      monsterMapId: PositiveWireInt
    })
  );
  var skillIndex = /* @__PURE__ */ __name((skill) => {
    return Number.isInteger(skill) && skill >= 0 && skill <= 5 ? skill : null;
  }, "skillIndex");
  var DEFAULT_SKILLS = [1, 2, 3, 4];
  var normalizeSkills = /* @__PURE__ */ __name((input) => {
    const skills = (input ?? []).filter((skill) => skillIndex(skill) !== null);
    return skills.length === 0 ? DEFAULT_SKILLS : skills;
  }, "normalizeSkills");
  var normalizeSkillInterval = /* @__PURE__ */ __name((interval) => {
    if (interval === void 0) return 150;
    return match(fromInput(interval), {
      onNone: /* @__PURE__ */ __name(() => 150, "onNone"),
      onSome: /* @__PURE__ */ __name((duration) => {
        const milliseconds = toMillis(duration);
        return Number.isFinite(milliseconds) ? Math.max(0, milliseconds) : 150;
      }, "onSome")
    });
  }, "normalizeSkillInterval");
  var makeCombat = /* @__PURE__ */ __name((bridge, antiCounter, host4, drops, _events, inventory, map8, monsters, player, _players, settings, temporary, wait) => {
    const consumableCasts = makeUnsafe5(1);
    const targetValue = readCombatTarget(bridge);
    const targetAuras = /* @__PURE__ */ __name((options) => targetValue.pipe(
      flatMap3((target2) => {
        if (target2 === null) return succeed7([]);
        return (target2.type === "monster" ? host4.read({
          kind: "monsterAuras",
          monsterMapId: target2.monsterMapId,
          auraKind: options?.kind ?? null
        }) : host4.read({
          kind: "playerAuras",
          entityId: target2.entityId,
          auraKind: options?.kind ?? null
        })).pipe(map5((auras) => auras.map(toLiveAura)));
      })
    ), "targetAuras");
    const getConsumableSkillItem = /* @__PURE__ */ __name(() => bridge.invoke("combat.getConsumableSkillItem", void 0, Consumable).pipe(
      map5(
        match({
          onNone: /* @__PURE__ */ __name(() => null, "onNone"),
          onSome: /* @__PURE__ */ __name((item) => {
            const itemId = item?.itemId ?? item?.ItemID;
            return itemId === void 0 ? null : { itemId, ready: item?.ready === true };
          }, "onSome")
        })
      )
    ), "getConsumableSkillItem");
    const prepareCombatProfile = fn2("Combat.prepareCombatProfile")(
      function* (profile) {
        const scope3 = yield* make6();
        return yield* gen2(function* () {
          const job = yield* startProfileJob(
            { profile, targetPriority: [] },
            profileJobHost
          ).pipe(provideService2(Scope, scope3));
          const slot = yield* getConsumableSkillItem();
          const classWarning = yield* getCombatProfileClassWarning(
            { inventory, player },
            { classNames: job.classNames }
          );
          const prepared = {
            ...slot === null ? {} : { skill5ItemId: slot.itemId },
            release: close(scope3, void_2),
            warnings: succeed7(
              [classWarning, ...job.warnings].filter((warning) => warning !== void 0)
            )
          };
          return prepared;
        }).pipe(
          onExit2(
            (exit3) => isFailure3(exit3) ? close(scope3, exit3) : void_3
          )
        );
      }
    );
    const getSkillCooldownRemainingMs = /* @__PURE__ */ __name((skill) => bridge.invoke("combat.getSkillCooldownRemaining", [skill], WireInt).pipe(
      map5(
        match({
          onNone: /* @__PURE__ */ __name(() => null, "onNone"),
          onSome: /* @__PURE__ */ __name((remaining) => Math.max(0, Math.trunc(remaining)), "onSome")
        })
      )
    ), "getSkillCooldownRemainingMs");
    const castConsumableOnMonster = /* @__PURE__ */ __name((selector, expectedItemId) => consumableCasts.withPermit(
      gen2(function* () {
        if (!Number.isInteger(expectedItemId) || expectedItemId <= 0) {
          return null;
        }
        let dispatch;
        let confirmation = null;
        const projectedSourceId = yield* host4.read({ kind: "selfEntityId" });
        const sourceId = projectedSourceId ?? (yield* bridge.invoke("player.getUserId", void 0, PositiveWireInt).pipe(map5(getOrNull)));
        if (sourceId === null) return null;
        yield* wait.forPacket(
          {
            command: "ct",
            direction: "server",
            predicate: /* @__PURE__ */ __name((candidate) => {
              if (dispatch === void 0) return false;
              const expectedActionId = dispatch.actionId;
              const matched = decodeCombatActionAcknowledgements(
                packetData(candidate)
              ).find(
                (candidate2) => candidate2.actionId === expectedActionId && candidate2.source.type === "player" && candidate2.source.id === sourceId
              );
              if (matched === void 0) return false;
              confirmation = {
                actionId: dispatch.actionId,
                monsterMapId: dispatch.monsterMapId,
                success: matched.outcome === 1
              };
              return true;
            }, "predicate"),
            encoding: "json"
          },
          {
            timeout: `${CONSUMABLE_CAST_CONFIRMATION_TIMEOUT_MS} millis`,
            trigger: gen2(function* () {
              const dispatchDeadline = yield* ConsumableCastDispatchDeadline;
              if (dispatchDeadline !== void 0 && (yield* currentTimeMillis2) > dispatchDeadline) {
                return false;
              }
              const result2 = yield* bridge.invoke(
                "combat.castConsumableOnMonster",
                [toMonsterSelector(selector), expectedItemId],
                ConsumableCastDispatch
              ).pipe(map5(getOrNull));
              if (result2 === null || result2.itemId !== expectedItemId) {
                return false;
              }
              dispatch = result2;
              return true;
            })
          }
        );
        return confirmation;
      })
    ), "castConsumableOnMonster");
    const canUseSkill = /* @__PURE__ */ __name((skill) => {
      const index = skillIndex(skill);
      return index === null ? succeed7(false) : getSkillCooldownRemainingMs(index).pipe(
        map5((remaining) => remaining === 0)
      );
    }, "canUseSkill");
    const waitForSkillReady = /* @__PURE__ */ __name((index) => wait.until(
      getSkillCooldownRemainingMs(index).pipe(
        flatMap3(
          (remaining) => remaining === null ? succeed7(false) : remaining > 0 ? sleep2(remaining).pipe(as2(false)) : sleep2("150 millis").pipe(
            andThen2(getSkillCooldownRemainingMs(index)),
            map5((confirmed) => confirmed === 0)
          )
        )
      ),
      { interval: "50 millis", timeout: "5 seconds" }
    ), "waitForSkillReady");
    const antiCounterActive = /* @__PURE__ */ __name((monsterMapId) => settings.isAntiCounterEnabled().pipe(
      flatMap3(
        (enabled) => enabled ? antiCounter.isActive(monsterMapId) : succeed7(false)
      )
    ), "antiCounterActive");
    const stopCombat2 = stopCombat(bridge);
    const resolveMonsterMapId = /* @__PURE__ */ __name((query) => monsters.getAvailable().pipe(
      map5(
        (available) => available.find((monster) => monster.matches(query))?.monsterMapId
      )
    ), "resolveMonsterMapId");
    const useSkill = /* @__PURE__ */ __name((skill, options) => {
      const index = skillIndex(skill);
      if (index === null) return succeed7(false);
      return gen2(function* () {
        const requestedTarget = options?.target;
        if (!(yield* player.isAlive())) return false;
        const selectedTarget = requestedTarget === void 0 ? yield* targetValue : null;
        const initialMonsterMapId = requestedTarget !== void 0 ? yield* resolveMonsterMapId(requestedTarget) : selectedTarget?.type === "monster" ? selectedTarget.monsterMapId : void 0;
        if (requestedTarget !== void 0 && initialMonsterMapId === void 0) {
          return false;
        }
        if (initialMonsterMapId !== void 0 && (yield* antiCounterActive(initialMonsterMapId))) {
          yield* stopCombat2;
          return false;
        }
        const ready = options?.waitUntilReady === true ? yield* waitForSkillReady(index) : yield* canUseSkill(index);
        if (!ready || !(yield* player.isAlive())) return false;
        const selectedTargetBeforeCast = requestedTarget === void 0 ? yield* targetValue : null;
        const guardedMonsterMapId = requestedTarget !== void 0 ? yield* resolveMonsterMapId(requestedTarget) : selectedTargetBeforeCast?.type === "monster" ? selectedTargetBeforeCast.monsterMapId : selectedTargetBeforeCast === null ? initialMonsterMapId : void 0;
        if (requestedTarget !== void 0 && guardedMonsterMapId === void 0) {
          return false;
        }
        if (guardedMonsterMapId !== void 0 && (yield* antiCounterActive(guardedMonsterMapId))) {
          if (requestedTarget !== void 0 || selectedTargetBeforeCast?.type === "monster") {
            yield* stopCombat2;
          }
          return false;
        }
        return yield* bridge.invoke(
          "combat.useSkill",
          [
            String(index),
            requestedTarget === void 0 || guardedMonsterMapId === void 0 ? null : toMonsterSelector(guardedMonsterMapId),
            options?.force === true
          ],
          Boolean3
        ).pipe(map5(getOrElse(() => false)));
      });
    }, "useSkill");
    const attack = /* @__PURE__ */ __name((selector) => {
      return gen2(function* () {
        if (!(yield* player.isAlive())) return false;
        const monster = yield* monsters.get(selector);
        if (monster !== null && (yield* antiCounterActive(monster.monsterMapId))) {
          yield* stopCombat2;
          return false;
        }
        return yield* bridge.invoke(
          "combat.attackMonster",
          [toMonsterSelector(selector)],
          Boolean3
        ).pipe(map5(getOrElse(() => false)));
      });
    }, "attack");
    const reachHuntTarget = /* @__PURE__ */ __name((target2) => {
      if (target2 === null || target2.cell === "") return succeed7(null);
      return player.jumpToCell(target2.cell).pipe(map5((reached) => reached ? target2 : null));
    }, "reachHuntTarget");
    const hunt = /* @__PURE__ */ __name((selector, options) => {
      return monsters.getAll().pipe(
        flatMap3((all3) => {
          const matches = all3.filter((monster) => monster.matches(selector));
          if (options?.preferMostMatches !== true || matches.length < 2) {
            return reachHuntTarget(matches[0] ?? null);
          }
          const cells = /* @__PURE__ */ new Map();
          for (const monster of matches) {
            cells.set(monster.cell, (cells.get(monster.cell) ?? 0) + 1);
          }
          const target2 = matches.reduce(
            (best, candidate) => (cells.get(candidate.cell) ?? 0) > (cells.get(best.cell) ?? 0) ? candidate : best
          );
          return reachHuntTarget(target2);
        })
      );
    }, "hunt");
    const makeKillProfileRuntime = /* @__PURE__ */ __name((selector, options) => {
      const profile = options?.profile;
      if (profile === void 0) return succeed7(null);
      return gen2(function* () {
        const job = yield* startProfileJob(
          { profile, targetPriority: [...options?.targetPriority ?? [], selector] },
          profileJobHost
        );
        const classWarning = yield* getCombatProfileClassWarning(
          { inventory, player },
          { classNames: job.classNames }
        );
        for (const warning of [classWarning, ...job.warnings].filter(
          (warning2) => warning2 !== void 0
        )) {
          yield* logWarning("Combat profile session degraded").pipe(
            annotateLogs({
              component: "combat",
              data: { profile: job.label, warning }
            })
          );
        }
        return job;
      });
    }, "makeKillProfileRuntime");
    const fight = /* @__PURE__ */ __name((selector, options, runtime2) => {
      if (runtime2 !== null) {
        return scoped2(
          acquireRelease2(
            sync2(() => profileJobHost.engageJob(runtime2.id, true)),
            () => sync2(() => profileJobHost.engageJob(runtime2.id, false))
          ).pipe(andThen2(never2))
        );
      }
      const skills = normalizeSkills(options?.skills);
      const skillInterval = normalizeSkillInterval(options?.skillInterval);
      const attackOrder = [...options?.targetPriority ?? [], selector];
      const selectTarget = gen2(function* () {
        const available = yield* monsters.getAvailable();
        const candidates = orderMonstersByPriority(available, attackOrder);
        for (const candidate of candidates) {
          if (!(yield* antiCounterActive(candidate.monsterMapId)))
            return candidate;
        }
        return null;
      });
      return forever3(
        gen2(function* () {
          if (!(yield* player.isAlive())) {
            yield* sleep2("250 millis");
            return;
          }
          const target2 = yield* selectTarget;
          if (target2 === null) {
            yield* stopCombat2;
            yield* sleep2("100 millis");
            return;
          }
          if (!(yield* attack(target2.monsterMapId))) {
            yield* sleep2("250 millis");
            return;
          }
          for (const skill of skills) {
            yield* useSkill(skill);
            yield* sleep2(skillInterval);
          }
        })
      );
    }, "fight");
    const killWithRuntime = /* @__PURE__ */ __name((selector, options, runtime2) => gen2(function* () {
      const startMap = yield* host4.read({ kind: "map" });
      const available = yield* monsters.getAvailable();
      const monster = available.find(
        (candidate) => candidate.matches(selector)
      );
      if (monster === void 0) return false;
      yield* map8.setSpawnPoint();
      const death = wait.forEvent(
        {
          monsterMapId: monster.monsterMapId,
          type: "monster-death"
        },
        {
          trigger: forkScoped2(fight(selector, options, runtime2)).pipe(
            as2(true)
          )
        }
      ).pipe(map5((event) => event !== null));
      const projectedEnd = wait.untilSome(
        all2([
          host4.read({
            kind: "monster",
            selector: { monsterMapId: monster.monsterMapId }
          }),
          host4.read({ kind: "map" })
        ]).pipe(
          map5(
            ([current, currentMap]) => current === null || currentMap.id !== startMap.id || currentMap.name !== startMap.name || currentMap.roomNumber !== startMap.roomNumber ? some2(false) : current.dead ? some2(true) : none2()
          )
        ),
        { interval: "250 millis" }
      );
      const result2 = raceFirst2(
        death,
        projectedEnd.pipe(map5((dead) => dead === true))
      );
      return yield* runtime2 === null ? result2 : raceFirst2(result2, runtime2.failed);
    }), "killWithRuntime");
    const kill = /* @__PURE__ */ __name((selector, options) => scoped2(
      gen2(function* () {
        const runtime2 = yield* makeKillProfileRuntime(selector, options);
        return yield* killWithRuntime(selector, options, runtime2);
      })
    ).pipe(ensuring2(stopCombat2)), "kill");
    const killFor = /* @__PURE__ */ __name((selector, item, requested, source, options) => {
      const wanted = Math.max(1, Math.trunc(requested ?? 1));
      return scoped2(
        gen2(function* () {
          const runtime2 = yield* makeKillProfileRuntime(selector, options);
          const loop = gen2(function* () {
            while (true) {
              if (yield* source.contains(item, wanted)) return true;
              if (yield* drops.contains(item)) {
                yield* drops.accept(item);
                if (yield* source.contains(item, wanted)) return true;
              }
              if (!(yield* killWithRuntime(selector, options, runtime2))) {
                yield* sleep2("100 millis");
              }
            }
          });
          return yield* runtime2 === null ? loop : raceFirst2(loop, runtime2.failed);
        })
      ).pipe(ensuring2(stopCombat2));
    }, "killFor");
    const cancelAutoAttack = /* @__PURE__ */ __name(() => bridge.invoke("combat.cancelAutoAttack", void 0, Void2).pipe(asVoid2), "cancelAutoAttack");
    const cancelTarget = /* @__PURE__ */ __name(() => bridge.invoke("combat.cancelTarget", void 0, Void2).pipe(asVoid2), "cancelTarget");
    const killForItem = /* @__PURE__ */ __name((selector, item, quantity, options) => killFor(selector, item, quantity, inventory, options), "killForItem");
    const killForTempItem = /* @__PURE__ */ __name((selector, item, quantity, options) => killFor(selector, item, quantity, temporary, options), "killForTempItem");
    const getTargetAura = /* @__PURE__ */ __name((name, options) => targetAuras(options).pipe(
      map5(
        (auras) => auras.find(
          (aura) => aura.name.localeCompare(name, void 0, {
            sensitivity: "accent"
          }) === 0
        ) ?? null
      )
    ), "getTargetAura");
    const hasTargetAura = /* @__PURE__ */ __name((name, options) => getTargetAura(name, options).pipe(map5((aura) => aura !== null)), "hasTargetAura");
    const getTarget = /* @__PURE__ */ __name(() => targetValue, "getTarget");
    const target = {
      auras: {
        get: getTargetAura,
        getAll: targetAuras,
        has: hasTargetAura
      },
      get: getTarget
    };
    return {
      attack,
      cancelAutoAttack,
      cancelTarget,
      canUseSkill,
      castConsumableOnMonster,
      exit: player.exitCombat,
      getConsumableSkillItem,
      getSkillCooldownRemainingMs,
      hunt,
      isAttackBlocked: antiCounterActive,
      kill,
      killForItem,
      killForTempItem,
      prepareCombatProfile,
      target,
      useSkill
    };
  }, "makeCombat");

  // packages/runtime/src/game/flash/api/Drops.ts
  var makeDrops = fnUntraced2(function* (bridge, host4, auth) {
    const operationHost = globalThis.__host;
    const get5 = /* @__PURE__ */ __name((selector) => readItem(host4, "drop", selector), "get");
    const accept = /* @__PURE__ */ __name((selector) => isSelectable(selector) ? operate({ kind: "acceptDrop", selector }, operationHost).pipe(
      map5((answer) => answer.value),
      catch_2(() => succeed7(false))
    ) : succeed7(false), "accept");
    const contains = /* @__PURE__ */ __name((selector) => get5(selector).pipe(map5((drop) => drop !== null)), "contains");
    const getAll = /* @__PURE__ */ __name(() => readItems(host4, "drop"), "getAll");
    const isCustomUiEnabled = /* @__PURE__ */ __name(() => bridge.invoke("drops.isUsingCustomDrops", void 0, Boolean3).pipe(map5(getOrElse(() => false))), "isCustomUiEnabled");
    const reject = /* @__PURE__ */ __name((selector) => gen2(function* () {
      if (!(yield* auth.isLoggedIn())) return false;
      if (!isSelectable(selector)) return false;
      const rejected2 = yield* host4.command({ kind: "rejectDrop", selector });
      return getOrElse(rejected2, () => false);
    }), "reject");
    const toggleUi = /* @__PURE__ */ __name(() => bridge.invoke("drops.toggleUi", void 0, Void2).pipe(asVoid2), "toggleUi");
    return {
      accept,
      contains,
      get: get5,
      getAll,
      isCustomUiEnabled,
      reject,
      toggleUi
    };
  });

  // packages/runtime/src/game/flash/contract/Event.ts
  var isProjectionEvent = /* @__PURE__ */ __name((event) => event.type !== "connection" && event.type !== "debug", "isProjectionEvent");
  var isScalar = /* @__PURE__ */ __name((value) => typeof value === "boolean" || typeof value === "number" || typeof value === "string", "isScalar");
  var matchesEvent = /* @__PURE__ */ __name((event, selector) => {
    if (selector === void 0) return true;
    if (selector === null || typeof selector !== "object" || Array.isArray(selector)) {
      return false;
    }
    const fields = Object.entries(selector);
    if (!fields.some(([key]) => key === "type")) return false;
    const record3 = event;
    return fields.every(
      ([key, value]) => isScalar(value) && record3[key] === value
    );
  }, "matchesEvent");

  // packages/runtime/src/game/flash/api/Events.ts
  var makeEvents = fnUntraced2(function* (gateway, wait) {
    const scope3 = yield* scope2;
    const runFork3 = runForkWith2(yield* context2());
    const stream = /* @__PURE__ */ __name((selector) => gateway.events.pipe(
      filter6(
        (event) => matchesEvent(event, selector)
      )
    ), "stream");
    const on = /* @__PURE__ */ __name(((selector, handler) => stream(selector).pipe(
      runForEach2(handler),
      forkIn2(scope3),
      map5((fiber2) => () => {
        runFork3(interrupt4(fiber2));
      })
    )), "on");
    const onProjection = /* @__PURE__ */ __name(((selector, handler) => on(
      selector,
      (event) => isProjectionEvent(event) ? handler(event) : void_3
    )), "onProjection");
    const once = wait.forEvent;
    const onceProjection = wait.forProjectionEvent;
    return {
      on,
      onProjection,
      once,
      onceProjection,
      stream
    };
  });

  // packages/runtime/src/game/flash/api/House.ts
  var makeHouse = /* @__PURE__ */ __name((bridge, host4) => {
    const contains = /* @__PURE__ */ __name((selector, quantity) => itemQuantity(host4, "house", selector).pipe(
      map5((owned) => owned >= normalizeItemQuantity(quantity))
    ), "contains");
    const getAll = /* @__PURE__ */ __name(() => readItems(host4, "house"), "getAll");
    const getSlots = /* @__PURE__ */ __name(() => bridge.invoke("house.getSlots", void 0, WireInt).pipe(map5(getOrElse(() => 0))), "getSlots");
    const getUsedSlots = /* @__PURE__ */ __name(() => host4.read({ kind: "items", container: "house" }).pipe(map5((items) => items.length)), "getUsedSlots");
    const get5 = /* @__PURE__ */ __name((selector) => readItem(host4, "house", selector), "get");
    const getAvailableSlots = /* @__PURE__ */ __name(() => zipWith2(
      getSlots(),
      getUsedSlots(),
      (slots, used) => Math.max(0, slots - used)
    ), "getAvailableSlots");
    return {
      contains,
      get: get5,
      getAll,
      getAvailableSlots,
      getSlots,
      getUsedSlots
    };
  }, "makeHouse");

  // packages/schemas/src/baseSchemas.ts
  var TrimmedString = String4.pipe(
    decodeTo2(
      String4,
      transformEffect2({
        decode: /* @__PURE__ */ __name((value) => succeed7(value.trim()), "decode"),
        encode: /* @__PURE__ */ __name((value) => succeed7(value.trim()), "encode")
      })
    )
  );
  var TrimmedNonEmptyString = TrimmedString.check(isNonEmpty());
  var NonNegativeInt = Int.check(
    isGreaterThanOrEqualTo2(0)
  );
  var PositiveInt = Int.check(isGreaterThanOrEqualTo2(1));
  var boundedInt = /* @__PURE__ */ __name((minimum, maximum) => Int.check(isBetween2({ minimum, maximum })), "boundedInt");

  // packages/schemas/src/scriptPath.ts
  var windowsReservedPathSegment = /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?$/i;
  var unsafeScriptPathCharacters = /[<>:"\\|?*]/;
  var hasControlCharacter = /* @__PURE__ */ __name((value) => Array.from(value).some((character) => character.charCodeAt(0) < 32), "hasControlCharacter");
  var parseScriptPathSegments = /* @__PURE__ */ __name((value) => {
    if (value === "" || value.startsWith("/") || value.includes("\\")) {
      return null;
    }
    const segments = value.split("/");
    return segments.every(
      (segment) => segment !== "" && segment !== "." && segment !== ".." && !segment.endsWith(".") && !segment.endsWith(" ") && !unsafeScriptPathCharacters.test(segment) && !hasControlCharacter(segment) && !windowsReservedPathSegment.test(segment)
    ) ? segments : null;
  }, "parseScriptPathSegments");

  // packages/schemas/src/scriptPackages.ts
  var hasScriptPathSyntax = /* @__PURE__ */ __name((value) => parseScriptPathSegments(value) !== null, "hasScriptPathSyntax");
  var isScriptPackageRepositorySubdirectory = /* @__PURE__ */ __name((value) => value.trim() === value && value.normalize("NFC") === value && hasScriptPathSyntax(value), "isScriptPackageRepositorySubdirectory");
  var ScriptPackageRepositorySubdirectorySchema = String4.check(
    makeFilter2(isScriptPackageRepositorySubdirectory, {
      expected: "a portable repository-relative path using forward slashes"
    })
  );
  var isReservedScriptPackageName = /* @__PURE__ */ __name((name) => name === "effect" || name === "lucent" || name.startsWith("lucent/"), "isReservedScriptPackageName");
  var ScriptPackageNameSchema = String4.check(
    makeFilter2(
      (name) => name.trim() === name && !isReservedScriptPackageName(name) && hasScriptPathSyntax(name),
      {
        expected: "a safe, case-sensitive, non-reserved package name"
      }
    )
  );
  var ScriptPackageDirectorySchema = String4.check(
    makeFilter2(
      (value) => value.trim() === value && value.normalize("NFC") === value && !value.includes("/") && hasScriptPathSyntax(value),
      { expected: "a portable single folder name" }
    )
  );
  var ScriptRelativePathSchema = String4.check(
    makeFilter2(hasScriptPathSyntax, {
      expected: "a safe relative path using forward slashes"
    })
  );
  var ScriptReferenceSchema = Union2([
    Struct({
      kind: Literal2("loose"),
      path: ScriptRelativePathSchema
    }),
    Struct({
      kind: Literal2("package"),
      packageName: ScriptPackageNameSchema,
      path: ScriptRelativePathSchema
    })
  ]);
  var ScriptPackageCompatibilitySchema = Union2([
    Struct({
      status: Literal2("compatible"),
      currentVersion: String4,
      requiredVersion: String4
    }),
    Struct({
      status: Literal2("incompatible"),
      currentVersion: String4,
      requiredVersion: String4
    }),
    Struct({
      status: Literal2("unknown"),
      currentVersion: String4,
      warning: optionalKey2(String4)
    })
  ]);
  var ScriptCatalogEntrySchema = Struct({
    name: String4,
    path: String4,
    relativePath: String4,
    packageName: optionalKey2(String4),
    reference: ScriptReferenceSchema
  });
  var ScriptPackageIntegritySchema = Literals([
    "modified",
    "unmanaged",
    "verified"
  ]);
  var ScriptPackageRepositorySourceFields = {
    repositoryUrl: String4,
    requestedRef: optionalKey2(String4),
    credentialId: optionalKey2(String4)
  };
  var ScriptPackageSourceSchema = Union2([
    Struct({
      kind: Literal2("repository"),
      ...ScriptPackageRepositorySourceFields,
      resolvedCommit: String4
    }),
    Struct({
      kind: Literal2("directory"),
      ...ScriptPackageRepositorySourceFields,
      // Local bundled copies have an exact tree without a resolved remote commit.
      resolvedCommit: optionalKey2(String4),
      resolvedTree: String4,
      subdirectory: ScriptPackageRepositorySubdirectorySchema
    })
  ]);
  var ScriptPackageRevisionSchema = Union2([
    Struct({ kind: Literal2("commit"), sha: String4 }),
    Struct({ kind: Literal2("tree"), sha: String4 })
  ]);
  var ScriptPackageUpdateStateSchema = Union2([
    Struct({ status: Literal2("unchecked") }),
    Struct({
      status: Literal2("current"),
      checkedAt: String4
    }),
    Struct({
      status: Literal2("available"),
      checkedAt: String4,
      revision: ScriptPackageRevisionSchema
    }),
    Struct({
      status: Literal2("unknown"),
      checkedAt: optionalKey2(String4),
      message: String4
    }),
    Struct({
      status: Literal2("rate-limited"),
      retryAt: String4,
      message: String4
    })
  ]);
  var ScriptPackageDependencyIssueSchema = Union2([
    Struct({
      reason: Literal2("missing"),
      packageName: String4,
      requiredVersion: String4
    }),
    Struct({
      reason: Literal2("version-mismatch"),
      installedVersion: String4,
      packageName: String4,
      requiredVersion: String4
    }),
    Struct({
      reason: Literal2("version-unavailable"),
      packageName: String4,
      requiredVersion: String4
    }),
    Struct({
      reason: Literal2("unavailable"),
      packageName: String4,
      requiredVersion: String4
    })
  ]);
  var ScriptPackageDependencyStatusSchema = Union2([
    Struct({ status: Literal2("ready") }),
    Struct({
      status: Literal2("blocked"),
      issues: ArraySchema(ScriptPackageDependencyIssueSchema)
    })
  ]);
  var ValidScriptPackageSchema = Struct({
    status: Literal2("valid"),
    compatibility: ScriptPackageCompatibilitySchema,
    dependencyStatus: ScriptPackageDependencyStatusSchema,
    description: optionalKey2(String4),
    integrity: ScriptPackageIntegritySchema,
    name: String4,
    path: String4,
    source: optionalKey2(ScriptPackageSourceSchema),
    update: ScriptPackageUpdateStateSchema,
    version: optionalKey2(String4),
    warning: optionalKey2(String4)
  });
  var InvalidScriptPackageSchema = Struct({
    status: Literal2("invalid"),
    diagnostic: String4,
    name: optionalKey2(String4),
    path: String4
  });
  var ScriptPackageSummarySchema = Union2([
    ValidScriptPackageSchema,
    InvalidScriptPackageSchema
  ]);
  var ScriptCatalogSchema = Struct({
    packages: ArraySchema(ScriptPackageSummarySchema),
    revision: String4,
    scripts: ArraySchema(ScriptCatalogEntrySchema)
  });
  var SCRIPT_CATALOG_PAGE_MAX_ENTRIES = 1024;
  var ScriptCatalogOverviewSchema = Struct({
    packages: ArraySchema(ScriptPackageSummarySchema),
    revision: String4,
    scriptCount: NonNegativeInt
  });
  var ScriptCatalogPageRequestSchema = Struct({
    limit: boundedInt(1, SCRIPT_CATALOG_PAGE_MAX_ENTRIES),
    offset: NonNegativeInt,
    query: String4,
    revision: String4
  });
  var ScriptCatalogPageSchema = Struct({
    entries: ArraySchema(ScriptCatalogEntrySchema),
    offset: NonNegativeInt,
    revision: String4,
    total: NonNegativeInt
  });
  var ScriptCatalogChangeSchema = Struct({
    revision: String4
  });
  var GitHubCredentialSummarySchema = Struct({
    id: String4,
    label: String4
  });
  var ScriptPackageNonEmptyStringSchema = String4.check(
    makeFilter2((value) => value.trim() !== "", {
      expected: "a non-empty string"
    })
  );
  var GitHubCredentialWriteSchema = Struct({
    id: optionalKey2(String4),
    label: ScriptPackageNonEmptyStringSchema,
    token: ScriptPackageNonEmptyStringSchema
  });
  var ScriptPackageInstallRequestSchema = Struct({
    credentialId: optionalKey2(String4),
    ref: optionalKey2(String4),
    replaceExisting: optionalKey2(Boolean3),
    repositoryUrl: ScriptPackageNonEmptyStringSchema,
    subdirectory: optionalKey2(ScriptPackageRepositorySubdirectorySchema)
  });
  var ScriptPackageUpdateRequestSchema = Struct({
    packageName: ScriptPackageNameSchema,
    replaceModified: optionalKey2(Boolean3)
  });
  var ScriptPackageRestoreRequestSchema = Struct({
    packageName: ScriptPackageNameSchema
  });
  var ScriptPackageRemoveRequestSchema = Struct({
    confirmModified: optionalKey2(Boolean3),
    packageName: ScriptPackageNameSchema
  });
  var ScriptPackageMutationResultSchema = Union2([
    Struct({
      status: Literal2("completed"),
      catalog: ScriptCatalogOverviewSchema,
      packageName: String4
    }),
    Struct({
      status: Literal2("unchanged"),
      catalog: ScriptCatalogOverviewSchema,
      packageName: String4
    }),
    Struct({
      status: Literal2("confirmation-required"),
      packageName: String4,
      reason: Literals(["existing-package", "local-modifications"])
    })
  ]);

  // packages/schemas/src/scriptInputs.ts
  var ScriptInputValueSchema = Union2([
    String4,
    Number6,
    Boolean3,
    ArraySchema(String4)
  ]);
  var ScriptInputTypeSchema = Literals([
    "string",
    "number",
    "boolean",
    "select",
    "multi-select"
  ]);
  var ScriptInputFieldBaseSchema = {
    key: String4,
    label: String4,
    description: optionalKey2(String4),
    required: optionalKey2(Boolean3)
  };
  var ScriptStringInputFieldSchema = Struct({
    ...ScriptInputFieldBaseSchema,
    type: Literal2("string"),
    default: optionalKey2(String4)
  });
  var ScriptNumberInputFieldSchema = Struct({
    ...ScriptInputFieldBaseSchema,
    type: Literal2("number"),
    default: optionalKey2(Number6)
  });
  var ScriptBooleanInputFieldSchema = Struct({
    ...ScriptInputFieldBaseSchema,
    type: Literal2("boolean"),
    default: optionalKey2(Boolean3)
  });
  var ScriptSelectInputFieldSchema = Struct({
    ...ScriptInputFieldBaseSchema,
    type: Literal2("select"),
    options: ArraySchema(String4),
    default: optionalKey2(String4)
  }).check(
    makeFilter2(
      ({ default: defaultValue, options }) => options.length > 0 && (defaultValue === void 0 || options.includes(defaultValue)),
      {
        expected: "select input options to be non-empty and contain the default value"
      }
    )
  );
  var ScriptMultiSelectInputFieldSchema = Struct({
    ...ScriptInputFieldBaseSchema,
    type: Literal2("multi-select"),
    options: ArraySchema(String4),
    default: optionalKey2(ArraySchema(String4))
  }).check(
    makeFilter2(
      ({ default: defaultValue, options }) => {
        const optionSet = new Set(options);
        return options.length > 0 && optionSet.size === options.length && (defaultValue === void 0 || new Set(defaultValue).size === defaultValue.length && defaultValue.every((value) => optionSet.has(value)));
      },
      {
        expected: "multi-select input options to be non-empty and unique, and contain every unique default value"
      }
    )
  );
  var ScriptInputFieldSchema = Union2([
    ScriptStringInputFieldSchema,
    ScriptNumberInputFieldSchema,
    ScriptBooleanInputFieldSchema,
    ScriptSelectInputFieldSchema,
    ScriptMultiSelectInputFieldSchema
  ]);
  var ScriptInputsDefinitionSchema = Struct({
    id: String4,
    fields: ArraySchema(ScriptInputFieldSchema)
  }).check(
    makeFilter2(
      ({ fields }) => new Set(fields.map((field2) => field2.key)).size === fields.length,
      { expected: "script input field keys to be unique" }
    )
  );
  var ScriptFileReferenceSchema = Struct({
    path: String4,
    name: String4,
    reference: optionalKey2(ScriptReferenceSchema)
  });
  var ScriptFileSchema = Struct({
    path: String4,
    name: String4,
    source: String4,
    inputs: NullOr(ScriptInputsDefinitionSchema),
    revision: String4,
    reference: optionalKey2(ScriptReferenceSchema)
  });
  var ScriptOpenFileResultSchema = Union2([
    Struct({
      canceled: Literal2(true)
    }),
    Struct({
      canceled: Literal2(false),
      file: ScriptFileSchema
    })
  ]);
  var ScriptSelectFileResultSchema = Union2([
    Struct({
      canceled: Literal2(true)
    }),
    Struct({
      canceled: Literal2(false),
      file: ScriptFileReferenceSchema
    })
  ]);
  var ScriptFileResolutionSchema = Union2([
    Struct({
      status: Literal2("found"),
      file: ScriptFileSchema
    }),
    Struct({
      status: Literal2("missing"),
      path: String4
    }),
    Struct({
      status: Literal2("failed"),
      path: String4,
      message: String4,
      detailsText: optionalKey2(String4)
    })
  ]);
  var ScriptInputValuesSchema = Record(
    String4,
    ScriptInputValueSchema
  );
  var defaultScriptInputValue = /* @__PURE__ */ __name((field2) => Object.hasOwn(field2, "default") ? field2.default : void 0, "defaultScriptInputValue");
  var normalizeValue = /* @__PURE__ */ __name((field2, value) => {
    switch (field2.type) {
      case "boolean":
        return typeof value === "boolean" ? value : void 0;
      case "number":
        return typeof value === "number" && Number.isFinite(value) ? value : void 0;
      case "multi-select": {
        if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) {
          return void 0;
        }
        const selected = new Set(value);
        return field2.options.filter((option2) => selected.has(option2));
      }
      case "select":
        return typeof value === "string" && field2.options.includes(value) ? value : void 0;
      case "string":
        return typeof value === "string" ? value : void 0;
    }
  }, "normalizeValue");
  var normalizeScriptInputValues = /* @__PURE__ */ __name((definition, rawValues = {}) => {
    const values = {};
    for (const field2 of definition.fields) {
      const normalized = normalizeValue(field2, rawValues[field2.key]);
      if (normalized !== void 0) {
        values[field2.key] = normalized;
        continue;
      }
      const fallback = defaultScriptInputValue(field2);
      if (fallback !== void 0) {
        const normalizedFallback = normalizeValue(field2, fallback);
        if (normalizedFallback !== void 0) {
          values[field2.key] = normalizedFallback;
        }
      }
    }
    return values;
  }, "normalizeScriptInputValues");
  var findMissingRequiredScriptInputs = /* @__PURE__ */ __name((definition, values) => definition.fields.flatMap((field2) => {
    if (field2.required !== true || !Object.hasOwn(values, field2.key)) {
      return field2.required === true ? [field2.key] : [];
    }
    const value = values[field2.key];
    return field2.type === "multi-select" && (!Array.isArray(value) || value.length === 0) ? [field2.key] : [];
  }), "findMissingRequiredScriptInputs");
  var validateScriptInputValues = /* @__PURE__ */ __name((definition, values) => {
    const normalized = normalizeScriptInputValues(definition, values);
    const fieldKeys = findMissingRequiredScriptInputs(definition, normalized);
    return fieldKeys.length === 0 ? { status: "ok", values: normalized } : { fieldKeys, status: "missing-required", values: normalized };
  }, "validateScriptInputValues");

  // packages/schemas/src/accountSettings.ts
  var ACCOUNT_SETTINGS_VERSION = 1;
  var MINIMUM_ROOM_NUMBER = 1;
  var MINIMUM_PRIVATE_ROOM = 1001;
  var MINIMUM_RANDOM_PRIVATE_ROOM = 1e4;
  var MAXIMUM_ROOM_NUMBER = 99999;
  var RoomNumberSchema = Int.check(
    isBetween2({
      minimum: MINIMUM_ROOM_NUMBER,
      maximum: MAXIMUM_ROOM_NUMBER
    })
  );
  var RoomPolicySchema = Union2([
    Struct({ kind: Literal2("public") }),
    Struct({ kind: Literal2("random-private") }),
    Struct({
      kind: Literal2("specific"),
      roomNumber: RoomNumberSchema
    })
  ]);
  var AccountScriptSettingsSchema = Struct({
    restartAfterReconnect: Boolean3,
    roomPolicy: RoomPolicySchema,
    safeStartStop: Boolean3
  });
  var AccountSettingsSchema = Struct({
    version: Literal2(ACCOUNT_SETTINGS_VERSION),
    scripts: AccountScriptSettingsSchema
  });
  var AccountScriptSettingsPatchSchema = Struct({
    restartAfterReconnect: optionalKey2(Boolean3),
    roomPolicy: optionalKey2(RoomPolicySchema),
    safeStartStop: optionalKey2(Boolean3)
  });
  var AccountSettingsPatchSchema = Struct({
    scripts: optionalKey2(AccountScriptSettingsPatchSchema)
  });
  var UnknownRecordSchema = Record(String4, Unknown2);
  var decodeRecord2 = decodeUnknownOption2(UnknownRecordSchema);
  var decodeBoolean = decodeUnknownOption2(Boolean3);
  var decodeRoomPolicy = decodeUnknownOption2(RoomPolicySchema);

  // packages/schemas/src/environment.ts
  var EnvironmentItemBuckets = [
    "ac-member",
    "ac-non-member",
    "non-ac-member",
    "non-ac-non-member"
  ];
  var EnvironmentItemBucketSchema = Literals(
    EnvironmentItemBuckets
  );
  var EnvironmentItemRulesSchema = Struct({
    buckets: ArraySchema(EnvironmentItemBucketSchema),
    rejectElse: Boolean3
  });
  var EnvironmentDropPolicySchema = Struct({
    /** Accept member-only AC-tagged items. */
    acceptAcMemberOnlyDrops: Boolean3,
    /** Accept non-member AC-tagged items. */
    acceptAcNonMemberDrops: Boolean3,
    /** Accept member-only non-AC items. */
    acceptNonAcMemberOnlyDrops: Boolean3,
    /** Accept non-member non-AC items. */
    acceptNonAcNonMemberDrops: Boolean3,
    /** Reject any unregistered drop that is not accepted by this policy. */
    rejectUnregisteredDrops: Boolean3
  });
  var EnvironmentQuestAutoRegisterOptionsSchema = Struct({
    requirements: Boolean3,
    rewards: Boolean3
  });
  var EnvironmentAutomationOptionsSchema = Struct({
    boosts: Boolean3,
    drops: Boolean3,
    quests: Boolean3
  });
  var EnvironmentAutomationCapabilities = [
    "boosts",
    "drops",
    "quests"
  ];
  var EnvironmentAutomationCapabilitySchema = Literals(
    EnvironmentAutomationCapabilities
  );
  var EnvironmentDropItemDataSchema = Struct({
    coins: Boolean3,
    memberOnly: Boolean3,
    name: String4
  });
  var EnvironmentStateSchema = Struct({
    automation: EnvironmentAutomationOptionsSchema,
    questIds: ArraySchema(PositiveInt),
    questAutoRegister: EnvironmentQuestAutoRegisterOptionsSchema,
    questRewards: Record(PositiveInt, PositiveInt),
    itemNames: ArraySchema(String4),
    itemNotificationNames: ArraySchema(String4),
    itemRules: EnvironmentItemRulesSchema,
    boosts: ArraySchema(String4)
  });
  var PositiveIntFromNumber = Number6.pipe(
    decodeTo2(
      PositiveInt,
      transform2({
        decode: Math.trunc,
        encode: /* @__PURE__ */ __name((value) => value, "encode")
      })
    )
  );
  var decodeQuestIdentifier = decodeUnknownOption2(PositiveIntFromNumber);
  var decodeTrimmedName = decodeUnknownOption2(TrimmedNonEmptyString);
  var isEnvironmentItemBucketValue = is2(EnvironmentItemBucketSchema);
  var normalizeEnvironmentItemRules = /* @__PURE__ */ __name((rules) => {
    const requested = new Set(rules.buckets.filter(isEnvironmentItemBucketValue));
    return {
      buckets: EnvironmentItemBuckets.filter((bucket) => requested.has(bucket)),
      rejectElse: rules.rejectElse
    };
  }, "normalizeEnvironmentItemRules");
  var environmentItemRulesToDropPolicy = /* @__PURE__ */ __name((rules) => {
    const buckets = new Set(normalizeEnvironmentItemRules(rules).buckets);
    return {
      acceptAcMemberOnlyDrops: buckets.has("ac-member"),
      acceptAcNonMemberDrops: buckets.has("ac-non-member"),
      acceptNonAcMemberOnlyDrops: buckets.has("non-ac-member"),
      acceptNonAcNonMemberDrops: buckets.has("non-ac-non-member"),
      rejectUnregisteredDrops: rules.rejectElse
    };
  }, "environmentItemRulesToDropPolicy");
  var isEnvironmentItemRules = is2(EnvironmentItemRulesSchema);
  var isEnvironmentQuestAutoRegisterOptions = is2(
    EnvironmentQuestAutoRegisterOptionsSchema
  );

  // packages/schemas/src/filesystem.ts
  var SCRIPT_FILE_SYSTEM_MAX_BYTES = 8 * 1024 * 1024;
  var FileSystemOperationSchema = Literals([
    "exists",
    "list",
    "read-json",
    "read-text",
    "remove",
    "write-json",
    "write-text"
  ]);
  var FileSystemErrorReasonSchema = Literals([
    "invalid-path",
    "not-found",
    "not-file",
    "not-directory",
    "directory-not-empty",
    "too-large",
    "too-many-entries",
    "invalid-utf8",
    "invalid-json",
    "not-json-serializable",
    "permission-denied",
    "busy",
    "session-closed",
    "unavailable"
  ]);
  var FileSystemErrorFields = {
    operation: FileSystemOperationSchema,
    path: optionalKey2(String4),
    reason: FileSystemErrorReasonSchema
  };
  var FileSystemFailureSchema = Struct(FileSystemErrorFields);
  var FileSystemError = class extends TaggedError3()(
    "FileSystemError",
    FileSystemErrorFields
  ) {
    static {
      __name(this, "FileSystemError");
    }
    get message() {
      const target = this.path === void 0 ? "" : ` at ${this.path}`;
      return `Filesystem ${this.operation} failed${target}: ${this.reason}.`;
    }
  };
  var FileSystemEntrySchema = Struct({
    kind: Literals(["file", "directory", "symbolic-link", "other"]),
    name: String4
  });

  // packages/runtime/src/game/EnhancementSelectors.ts
  var EnhancementSelectorSchema = Struct({
    enhancement: TrimmedNonEmptyString,
    special: optionalKey2(TrimmedNonEmptyString)
  });
  var enhancementSlots = ["cape", "class", "helm", "weapon"];
  var EnhancementSlotSchema = TrimmedNonEmptyString.pipe(
    decodeTo2(
      Literals(enhancementSlots),
      transform2({
        decode: /* @__PURE__ */ __name((value) => value.toLowerCase(), "decode"),
        encode: /* @__PURE__ */ __name((value) => value, "encode")
      })
    )
  );
  var EquipEnhancementSelectorSchema = Struct({
    ...EnhancementSelectorSchema.fields,
    slot: optionalKey2(EnhancementSlotSchema)
  });

  // packages/runtime/src/game/flash/api/Inventory.ts
  var WearResponse = Struct({
    ItemID: PositiveWireInt,
    success: WireBoolean,
    uid: PositiveWireInt
  });
  var EquipResponse = Struct({
    ItemID: PositiveWireInt,
    uid: PositiveWireInt
  });
  var decodeWearResponse = decodeUnknownOption2(WearResponse);
  var decodeEquipResponse = decodeUnknownOption2(EquipResponse);
  var decodeEquipEnhancementSelector = decodeUnknownOption2(
    EquipEnhancementSelectorSchema
  );
  var isDirectInventoryConsumable = /* @__PURE__ */ __name((link2) => {
    const normalized = link2.trim().toLowerCase();
    return normalized === "elixir" || normalized === "tonic";
  }, "isDirectInventoryConsumable");
  var isDirectInventoryUseItem = /* @__PURE__ */ __name((category, link2) => category.trim().toLowerCase() === "serveruse" || isDirectInventoryConsumable(link2), "isDirectInventoryUseItem");
  var slotMethods = {
    bag: "inventory.getSlots",
    misc: "inventory.getMiscSlots"
  };
  var makeInventory = /* @__PURE__ */ __name((bridge, host4, house, wait) => {
    const consumableEquipment = makeUnsafe5(1);
    const getAll = /* @__PURE__ */ __name(() => readItems(host4, "inventory"), "getAll");
    const get5 = /* @__PURE__ */ __name((selector) => readItem(host4, "inventory", selector), "get");
    const getSlots = fn2("Inventory.getSlots")(function* (pool = "bag") {
      if (!(yield* isHydrated(host4, "inventory"))) return 0;
      return yield* bridge.invoke(slotMethods[pool], void 0, WireInt).pipe(map5(getOrElse(() => 0)));
    });
    const getUsedSlots = /* @__PURE__ */ __name((pool = "bag") => host4.read({ kind: "items", container: "inventory" }).pipe(
      map5(
        (items) => items.filter((item) => item.pool === pool).length
      )
    ), "getUsedSlots");
    const canAccept = fn2("Inventory.canAccept")(function* (item, replacing) {
      const pool = item.pool;
      if (pool === "class") return true;
      if (replacing !== void 0 && (pool === "house" || replacing.pool === pool)) {
        return true;
      }
      if (pool === "house") return (yield* house.getAvailableSlots()) > 0;
      const current = yield* get5(item.itemId);
      if (replacing === void 0 && current !== null && current.quantity < (current.maxStack ?? 1)) {
        return true;
      }
      if (pool === "misc" && current !== null) return true;
      return (yield* getAvailableSlots(pool)) > 0;
    });
    const contains = /* @__PURE__ */ __name((selector, quantity) => get5(selector).pipe(
      map5(
        (item) => item !== null && item.quantity >= normalizeItemQuantity(quantity)
      )
    ), "contains");
    const canUseMemberItem = fn2("Inventory.canUseMemberItem")(function* (memberOnly) {
      if (!memberOnly) return true;
      return yield* bridge.invoke("player.isMember", void 0, Boolean3).pipe(map5(getOrElse(() => false)));
    });
    const getLocalUserId = fn2("Inventory.getLocalUserId")(function* () {
      const cached2 = yield* host4.read({ kind: "selfEntityId" });
      return cached2 === null ? yield* bridge.invoke("player.getUserId", void 0, PositiveWireInt) : some2(cached2);
    });
    const wearItem = fn2("Inventory.wearItem")(function* (itemId) {
      const userId = yield* getLocalUserId();
      if (isNone2(userId)) return false;
      if (!(yield* wait.forGameAction("wearItem"))) return false;
      let response;
      const wearPacket = yield* wait.forPacket(
        {
          command: "wearItem",
          direction: "extension",
          predicate: /* @__PURE__ */ __name((candidate) => {
            const decoded = decodeWearResponse(packetData(candidate));
            if (isNone2(decoded) || decoded.value.ItemID !== itemId || decoded.value.uid !== userId.value) {
              return false;
            }
            response = decoded.value;
            return true;
          }, "predicate"),
          encoding: "json"
        },
        {
          timeout: "5 seconds",
          trigger: bridge.invoke("inventory.wear", [{ itemId }], Boolean3).pipe(map5(getOrElse(() => false)))
        }
      );
      return wearPacket !== null && response?.success === true;
    });
    const wearEffect = fn2("Inventory.wear")(function* (selector) {
      const item = yield* get5(selector);
      if (item === null || !item.wearable) return false;
      if (item.worn) return true;
      if (!(yield* canUseMemberItem(item.memberOnly))) return false;
      return yield* wearItem(item.itemId);
    });
    const wear = /* @__PURE__ */ __name((selector) => wearEffect(selector), "wear");
    const operationHost = globalThis.__host;
    const equipConsumable = /* @__PURE__ */ __name((itemId, claim) => scoped2(gen2(function* () {
      const deadline = Date.now() + 5e3;
      const permit = yield* acquireRelease2(
        consumableEquipment.take(1).pipe(timeoutOption2("5 seconds")),
        (permit2) => isSome2(permit2) ? consumableEquipment.release(1) : void_3,
        { interruptible: true }
      );
      if (isNone2(permit)) return false;
      return yield* operate(
        claim === void 0 ? { kind: "equipConsumable", selector: itemId } : { kind: "prepareConsumableClaim", claim },
        operationHost,
        deadline
      ).pipe(
        map5((answer) => answer.value),
        catch_2(() => succeed7(false))
      );
    })), "equipConsumable");
    const equipEffect = fn2("Inventory.equip")(function* (selector, options, claim) {
      const item = yield* get5(selector);
      if (item === null) return false;
      if (item.category === "Item" && isDirectInventoryConsumable(item.link)) {
        return false;
      }
      const needsEquip = item.category === "Item" || !item.equipped;
      const needsWear = (options?.wear ?? true) && item.wearable && !item.worn;
      if (!needsEquip && !needsWear) return true;
      if (!(yield* canUseMemberItem(item.memberOnly))) return false;
      if (needsEquip) {
        if (!(yield* wait.forGameAction("equipItem"))) return false;
        if (item.category === "Item") {
          const equipped = yield* equipConsumable(item.itemId, claim);
          if (!equipped) return false;
        } else {
          const userId = yield* getLocalUserId();
          if (isNone2(userId)) return false;
          const equipPacket = yield* wait.forPacket(
            {
              command: "equipItem",
              direction: "extension",
              predicate: /* @__PURE__ */ __name((candidate) => {
                const decoded = decodeEquipResponse(packetData(candidate));
                return isSome2(decoded) && decoded.value.ItemID === item.itemId && decoded.value.uid === userId.value;
              }, "predicate"),
              encoding: "json"
            },
            {
              timeout: "5 seconds",
              trigger: bridge.invoke(
                "inventory.equip",
                [{ itemId: item.itemId }],
                Boolean3
              ).pipe(map5(getOrElse(() => false)))
            }
          );
          if (equipPacket === null) return false;
        }
      }
      return needsWear ? yield* wearItem(item.itemId) : true;
    });
    const equip = /* @__PURE__ */ __name((selector, options) => equipEffect(selector, options), "equip");
    const use = fn2("Inventory.use")(function* (selector) {
      const item = yield* get5(selector);
      if (item === null || !isDirectInventoryUseItem(item.category, item.link)) {
        return false;
      }
      if (!(yield* canUseMemberItem(item.memberOnly))) return false;
      const startingQuantity = item.quantity;
      const sent = yield* bridge.invoke("inventory.use", [{ itemId: item.itemId }], Boolean3).pipe(map5(getOrElse(() => false)));
      if (!sent) return false;
      return yield* wait.until(
        get5(item.itemId).pipe(
          map5(
            (current) => current === null || current.quantity < startingQuantity
          )
        ),
        { timeout: "5 seconds" }
      );
    });
    const equipByEnhancementEffect = fn2("Inventory.equipByEnhancement")(
      function* (selector, options) {
        const decoded = decodeEquipEnhancementSelector(selector);
        if (isNone2(decoded)) return false;
        const normalized = decoded.value;
        if (normalized.slot === void 0 && normalized.special === void 0 && normalized.enhancement.toLowerCase() === "forge") {
          return false;
        }
        const items = yield* getAll();
        let target;
        for (const item of items) {
          if (item.category.trim().toLowerCase() === "enhancement") continue;
          const resolution = resolveEnhancementStrategy(
            item,
            normalized.enhancement,
            item.enhancement?.level ?? 0,
            normalized.special
          );
          if (!resolution.ok || normalized.slot !== void 0 && resolution.strategy.slot !== normalized.slot || !matchesAppliedEnhancement(item, resolution.strategy)) {
            continue;
          }
          if (target === void 0 || (item.enhancement?.level ?? 0) > (target.enhancement?.level ?? 0)) {
            target = item;
          }
        }
        if (target === void 0) return false;
        return yield* equip(target.itemId, options);
      }
    );
    const equipByEnhancement = /* @__PURE__ */ __name((selector, options) => equipByEnhancementEffect(selector, options), "equipByEnhancement");
    const getAvailableSlots = /* @__PURE__ */ __name((pool = "bag") => zipWith2(
      getSlots(pool),
      getUsedSlots(pool),
      (slots, used) => Math.max(0, slots - used)
    ), "getAvailableSlots");
    const unequipConsumable = fn2("Inventory.unequipConsumable")(
      function* (selector) {
        const item = yield* get5(selector);
        if (item === null || item.category !== "Item") return false;
        const sent = yield* bridge.invoke(
          "inventory.unequipConsumable",
          [{ itemId: item.itemId }],
          Boolean3
        ).pipe(map5(getOrElse(() => false)));
        if (!sent) return false;
        yield* host4.command({ kind: "refreshEquippedConsumable" });
        const current = yield* get5(item.itemId);
        return current?.equipped !== true;
      },
      consumableEquipment.withPermit,
      timeoutOption2("5 seconds"),
      map5(getOrElse(() => false))
    );
    return {
      canAccept,
      contains,
      equip,
      equipByEnhancement,
      equipClaimedConsumable: /* @__PURE__ */ __name((itemId, claim) => equipEffect(itemId, void 0, claim), "equipClaimedConsumable"),
      get: get5,
      getAll,
      getAvailableSlots,
      getSlots,
      getUsedSlots,
      unequipConsumable,
      use,
      wear
    };
  }, "makeInventory");

  // packages/runtime/src/game/flash/api/Map.ts
  var makeMap = /* @__PURE__ */ __name((bridge, host4, wait) => {
    const isLoaded = /* @__PURE__ */ __name(() => bridge.invoke("world.isLoaded", void 0, Boolean3).pipe(map5(getOrElse(() => false))), "isLoaded");
    const getCellPads = /* @__PURE__ */ __name(() => host4.command({ kind: "refreshCellPads" }).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => host4.read({ kind: "cellPads" }), "onNone"),
          onSome: succeed7
        })
      )
    ), "getCellPads");
    const getCells = /* @__PURE__ */ __name(() => host4.command({ kind: "refreshCells" }).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => host4.read({ kind: "cells" }), "onNone"),
          onSome: succeed7
        })
      )
    ), "getCells");
    const getMap = /* @__PURE__ */ __name(() => host4.read({ kind: "map" }), "getMap");
    const getId = /* @__PURE__ */ __name(() => getMap().pipe(map5((map8) => map8.id)), "getId");
    const getMapItem = /* @__PURE__ */ __name((itemId) => {
      if (!Number.isSafeInteger(itemId) || itemId <= 0) return void_3;
      return wait.forGameAction("getMapItem").pipe(
        flatMap3(
          (ready) => ready ? bridge.invoke("world.getMapItem", [itemId], Void2) : succeed7(none2())
        ),
        asVoid2
      );
    }, "getMapItem");
    const getName = /* @__PURE__ */ __name(() => getMap().pipe(map5((map8) => map8.name)), "getName");
    const getRoomNumber = /* @__PURE__ */ __name(() => getMap().pipe(map5((map8) => map8.roomNumber)), "getRoomNumber");
    const loadSwf = /* @__PURE__ */ __name((swf) => bridge.invoke("world.loadSwf", [swf], Void2).pipe(asVoid2), "loadSwf");
    const reload = /* @__PURE__ */ __name(() => bridge.invoke("world.reload", void 0, Void2).pipe(asVoid2), "reload");
    const setSpawnPoint = /* @__PURE__ */ __name((options) => bridge.invoke("world.setSpawnPoint", [options?.cell, options?.pad], Void2).pipe(asVoid2), "setSpawnPoint");
    return {
      getCellPads,
      getCells,
      getId,
      getMapItem,
      getName,
      getRoomNumber,
      isLoaded,
      loadSwf,
      reload,
      setSpawnPoint
    };
  }, "makeMap");

  // packages/runtime/src/game/flash/contract/payload/MonsterDrops.ts
  var MonsterDropsPayload = Struct({
    MonMapID: PositiveWireInt,
    items: Record(String4, Unknown2)
  });
  var decodeMonsterDrops = decodeUnknownOption2(MonsterDropsPayload);

  // packages/runtime/src/game/flash/api/Monsters.ts
  var monsterDropRequestSpacing = spaced("350 millis").pipe(
    jittered
  );
  var makeMonsters = fnUntraced2(function* (bridge, host4, wait) {
    const readMonster = /* @__PURE__ */ __name((selector) => isSelectable(selector) ? host4.read({ kind: "monster", selector }).pipe(
      map5(
        (monster) => monster === null ? null : toLiveMonster(monster)
      )
    ) : succeed7(null), "readMonster");
    const readMonsters = /* @__PURE__ */ __name(() => host4.read({ kind: "monsters" }).pipe(map5((monsters) => monsters.map(toLiveMonster))), "readMonsters");
    const readDrops = /* @__PURE__ */ __name((monsterMapId) => host4.read({ kind: "monsterDrops", monsterMapId }), "readDrops");
    const requests = yield* unbounded2();
    const inFlight = /* @__PURE__ */ new Map();
    yield* addFinalizer3(() => shutdown2(requests));
    const requestKey = fn2("Monsters.requestKey")(function* (monsterMapId) {
      const map8 = yield* host4.read({ kind: "map" });
      return `${map8.id}:${map8.name}:${map8.roomNumber}:${monsterMapId}`;
    });
    const isInCurrentCell = fn2("Monsters.isInCurrentCell")(function* (monster) {
      const player = yield* host4.read({ kind: "me" });
      return player !== null && monster.isInCell(player.cell);
    });
    const executeRequest = fn2("Monsters.executeDropRequest")(function* (request) {
      const operation = gen2(function* () {
        if ((yield* requestKey(request.monsterMapId)) !== request.key) return;
        if ((yield* readDrops(request.monsterMapId)) !== null) return;
        const monster = yield* readMonster(request.monsterMapId);
        if (monster === null || !(yield* isInCurrentCell(monster))) return;
        yield* wait.forPacket(
          {
            command: "monsterDrops",
            direction: "extension",
            predicate: /* @__PURE__ */ __name((packet) => {
              const decoded = decodeMonsterDrops(packetData(packet));
              return isSome2(decoded) && decoded.value.MonMapID === request.monsterMapId;
            }, "predicate"),
            encoding: "json"
          },
          {
            timeout: "3 seconds",
            trigger: bridge.invoke(
              "world.requestMonsterDrops",
              [request.monsterMapId],
              Boolean3
            ).pipe(map5(getOrElse(() => false)))
          }
        );
      });
      yield* operation.pipe(
        ensuring2(
          succeed5(request.done, void 0).pipe(
            andThen2(
              sync2(() => {
                if (inFlight.get(request.key) === request.done) {
                  inFlight.delete(request.key);
                }
              })
            ),
            asVoid2
          )
        )
      );
    });
    yield* fromQueue(requests).pipe(
      schedule3(monsterDropRequestSpacing),
      mapEffect2((request) => executeRequest(request), {
        concurrency: "unbounded"
      }),
      runDrain2,
      forkScoped2
    );
    const ensureDrops = fn2("Monsters.ensureDrops")(function* (monster) {
      if ((yield* readDrops(monster.monsterMapId)) !== null) return;
      if (!(yield* isInCurrentCell(monster))) return;
      const key = yield* requestKey(monster.monsterMapId);
      const active = inFlight.get(key);
      if (active !== void 0) {
        yield* _await(active);
        return;
      }
      const done4 = makeUnsafe2();
      inFlight.set(key, done4);
      yield* offer(requests, {
        done: done4,
        key,
        monsterMapId: monster.monsterMapId
      });
      yield* _await(done4);
    });
    const ensureAllDrops = /* @__PURE__ */ __name((monsters) => forEach2(monsters, ensureDrops, {
      concurrency: "unbounded",
      discard: true
    }), "ensureAllDrops");
    const getProjected = /* @__PURE__ */ __name((selector) => readMonster(selector).pipe(
      flatMap3(
        (current) => current !== null || !isSelectable(selector) ? succeed7(current) : host4.command({ kind: "hydrateMonster", selector }).pipe(
          map5(
            (hydrated) => isSome2(hydrated) && hydrated.value !== null ? toLiveMonster(hydrated.value) : null
          )
        )
      )
    ), "getProjected");
    const getAvailableProjected = /* @__PURE__ */ __name(() => bridge.invoke(
      "world.getAvailableMonsterMapIds",
      void 0,
      ArraySchema(Number6)
    ).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => succeed7([]), "onNone"),
          onSome: /* @__PURE__ */ __name((ids) => readMonsters().pipe(
            map5((monsters) => {
              const available = new Set(ids);
              return monsters.filter(
                (monster) => available.has(monster.monsterMapId)
              );
            })
          ), "onSome")
        })
      )
    ), "getAvailableProjected");
    const ensureCurrentCell = fn2("Monsters.ensureCurrentCell")(function* (monsters) {
      const player = yield* host4.read({ kind: "me" });
      if (player === null) return;
      yield* ensureAllDrops(
        monsters.filter((monster) => monster.isInCell(player.cell))
      );
    });
    const reread = /* @__PURE__ */ __name((monsters) => {
      const ids = new Set(monsters.map((monster) => monster.monsterMapId));
      return readMonsters().pipe(
        map5(
          (current) => current.filter((monster) => ids.has(monster.monsterMapId))
        )
      );
    }, "reread");
    const get5 = /* @__PURE__ */ __name((selector) => getProjected(selector).pipe(
      flatMap3(
        (monster) => monster === null ? succeed7(null) : ensureDrops(monster).pipe(
          andThen2(readMonster(monster.monsterMapId))
        )
      )
    ), "get");
    const getAll = /* @__PURE__ */ __name(() => readMonsters().pipe(
      flatMap3(ensureCurrentCell),
      andThen2(readMonsters())
    ), "getAll");
    const getAvailable = /* @__PURE__ */ __name(() => getAvailableProjected().pipe(
      tap2(ensureAllDrops),
      flatMap3(reread)
    ), "getAvailable");
    const isAvailable = /* @__PURE__ */ __name((selector) => getProjected(selector).pipe(
      flatMap3(
        (monster) => monster === null ? succeed7(false) : bridge.invoke(
          "world.isMonsterAvailable",
          [monster.monsterMapId],
          Boolean3
        ).pipe(map5(getOrElse(() => false)))
      )
    ), "isAvailable");
    return {
      ensureCurrentCell,
      api: {
        get: get5,
        getAll,
        getAvailable,
        isAvailable
      },
      lookup: {
        get: getProjected,
        getAll: readMonsters,
        getAvailable: getAvailableProjected
      }
    };
  });

  // packages/runtime/src/game/flash/api/Packet.ts
  var placeholders = [
    "MAP_ID",
    "ROOM_NUMBER",
    "MAP_NAME",
    "PLAYER_NAME"
  ];
  var makePacket = fnUntraced2(function* (gateway, host4, wait) {
    const scope3 = yield* scope2;
    const runFork3 = runForkWith2(yield* context2());
    const stream = /* @__PURE__ */ __name((selector) => gateway.packets.pipe(
      filter6(
        (packet) => matchesPacket(packet, selector)
      )
    ), "stream");
    const resolve3 = /* @__PURE__ */ __name((packet) => gen2(function* () {
      if (!placeholders.some((token) => packet.includes(`{${token}}`))) {
        return packet;
      }
      const map8 = yield* host4.read({ kind: "map" });
      const player = yield* host4.read({ kind: "me" });
      return packet.replaceAll("{MAP_ID}", String(map8.id)).replaceAll("{ROOM_NUMBER}", String(map8.roomNumber)).replaceAll("{MAP_NAME}", map8.name).replaceAll("{PLAYER_NAME}", player?.username ?? "");
    }), "resolve");
    const on = /* @__PURE__ */ __name(((selector, handler) => stream(selector).pipe(
      runForEach2(handler),
      forkIn2(scope3),
      map5((fiber2) => () => {
        runFork3(interrupt4(fiber2));
      })
    )), "on");
    const onRaw = /* @__PURE__ */ __name((handler) => gateway.rawPackets.pipe(
      runForEach2(handler),
      forkIn2(scope3),
      map5((fiber2) => () => {
        runFork3(interrupt4(fiber2));
      })
    ), "onRaw");
    const once = wait.forPacket;
    const sendToClient = /* @__PURE__ */ __name((packet, encoding) => resolve3(packet).pipe(
      flatMap3((resolved) => gateway.sendToClient(resolved, encoding))
    ), "sendToClient");
    const sendToServer = /* @__PURE__ */ __name((packet, encoding) => resolve3(packet).pipe(
      flatMap3((resolved) => gateway.sendToServer(resolved, encoding))
    ), "sendToServer");
    return {
      on,
      onRaw,
      once,
      sendToClient,
      sendToServer,
      stream
    };
  });

  // packages/runtime/src/game/flash/domain/CombatExit.ts
  var cellKey = /* @__PURE__ */ __name((cell) => cell.trim().toLowerCase(), "cellKey");
  var transitionCells = /* @__PURE__ */ new Set(["", "blank", "wait"]);
  var combatExitCells = /* @__PURE__ */ __name((cells, monsters, currentCell) => {
    const aggressiveCells = new Set(
      monsters.filter((monster) => monster.aggressive).map((monster) => cellKey(monster.cell))
    );
    const occupiedCells = new Set(
      monsters.map((monster) => cellKey(monster.cell))
    );
    const current = cellKey(currentCell);
    const rank = /* @__PURE__ */ __name((cell) => cellKey(cell) === current ? 0 : occupiedCells.has(cellKey(cell)) ? 2 : 1, "rank");
    return cells.filter(
      (cell) => !transitionCells.has(cellKey(cell)) && !aggressiveCells.has(cellKey(cell))
    ).toSorted((left, right) => rank(left) - rank(right));
  }, "combatExitCells");

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Random.js
  var Random2 = Random;
  var randomWith = /* @__PURE__ */ __name((f) => withFiber2((fiber2) => succeed7(f(fiber2.getRef(Random2)))), "randomWith");
  var nextIntBetween = /* @__PURE__ */ __name((min3, max3, options) => {
    const extra = options?.halfOpen === true ? 0 : 1;
    return randomWith((r) => {
      const minInt = Math.ceil(min3);
      const maxInt = Math.floor(max3);
      return Math.floor(r.nextDoubleUnsafe() * (maxInt - minInt + extra)) + minInt;
    });
  }, "nextIntBetween");

  // packages/runtime/src/game/flash/domain/MapTarget.ts
  var minimumPrivateRoom = MINIMUM_PRIVATE_ROOM;
  var minimumRandomPrivateRoom = MINIMUM_RANDOM_PRIVATE_ROOM;
  var maximumRoom = MAXIMUM_ROOM_NUMBER;
  var splitRoom = /* @__PURE__ */ __name((map8) => {
    const trimmed = map8.trim();
    const separator = trimmed.indexOf("-");
    if (separator <= 0 || separator === trimmed.length - 1) {
      return { map: trimmed, name: trimmed };
    }
    return {
      map: trimmed,
      name: trimmed.slice(0, separator),
      roomToken: trimmed.slice(separator + 1)
    };
  }, "splitRoom");
  var parseRoom = /* @__PURE__ */ __name((value) => {
    if (!/^\d+$/u.test(value)) return void 0;
    const room = Number(value);
    return Number.isSafeInteger(room) && room >= 1 ? room : void 0;
  }, "parseRoom");
  var randomPrivateRoom = nextIntBetween(
    minimumRandomPrivateRoom,
    maximumRoom
  );
  var isPrivateRoom = /* @__PURE__ */ __name((room) => Number.isSafeInteger(room) && room >= minimumPrivateRoom && room <= maximumRoom, "isPrivateRoom");
  var withRoomNumber = /* @__PURE__ */ __name((map8, room) => {
    const target = splitRoom(map8);
    if (target.roomToken !== void 0 && parseRoom(target.roomToken) !== void 0) {
      return target.map;
    }
    return `${target.name}-${Math.max(1, Math.trunc(room))}`;
  }, "withRoomNumber");
  var applyRoomPolicy = fn2("applyRoomPolicy")(function* (map8, policy) {
    const trimmed = map8.trim();
    if (trimmed === "" || policy.kind === "public") return map8;
    const room = policy.kind === "specific" ? policy.roomNumber : yield* randomPrivateRoom;
    return withRoomNumber(trimmed, room);
  });
  var roomPolicyAcceptsRoom = /* @__PURE__ */ __name((policy, roomNumber) => policy.kind === "public" || (policy.kind === "random-private" ? isPrivateRoom(roomNumber) : roomNumber === policy.roomNumber), "roomPolicyAcceptsRoom");
  var parseMapTarget = /* @__PURE__ */ __name((map8) => gen2(function* () {
    const target = splitRoom(map8);
    if (target.roomToken === void 0) {
      return {
        map: target.map,
        name: target.name,
        requireExactRoom: false
      };
    }
    const fixedRoom = parseRoom(target.roomToken);
    const roomNumber = fixedRoom ?? (yield* randomPrivateRoom);
    const requireExactRoom = fixedRoom === void 0 || fixedRoom >= minimumPrivateRoom && fixedRoom <= maximumRoom;
    return {
      map: fixedRoom === void 0 ? `${target.name}-${roomNumber}` : target.map,
      name: target.name,
      requireExactRoom,
      roomNumber
    };
  }), "parseMapTarget");

  // packages/runtime/src/game/flash/api/Player.ts
  var FactionPayload = Struct({
    FactionID: PositiveWireInt,
    iRank: optionalKey2(WireInt),
    iRep: optionalKey2(WireInt),
    sName: String4
  });
  var FactionPayloads = ArraySchema(FactionPayload);
  var OutfitPayload = Struct({
    None: optionalKey2(NonNegativeWireInt),
    Weapon: optionalKey2(NonNegativeWireInt),
    ar: optionalKey2(NonNegativeWireInt),
    ba: optionalKey2(NonNegativeWireInt),
    co: optionalKey2(NonNegativeWireInt),
    colors: optionalKey2(
      Struct({
        accessory: optionalKey2(WireInt),
        base: optionalKey2(WireInt),
        eye: optionalKey2(WireInt),
        hair: optionalKey2(WireInt),
        skin: optionalKey2(WireInt),
        trim: optionalKey2(WireInt)
      })
    ),
    he: optionalKey2(NonNegativeWireInt),
    mi: optionalKey2(NonNegativeWireInt),
    name: String4,
    pe: optionalKey2(NonNegativeWireInt)
  });
  var OutfitPayloads = ArraySchema(OutfitPayload);
  var sameText = /* @__PURE__ */ __name((left, right) => left.localeCompare(right, void 0, { sensitivity: "accent" }) === 0, "sameText");
  var findClassItem = /* @__PURE__ */ __name((items, query) => items.find(
    (item) => item.classItem && (query === void 0 ? item.equipped : item.matches(query))
  ), "findClassItem");
  var toFaction = /* @__PURE__ */ __name((payload) => new LiveFaction({
    id: payload.FactionID,
    name: payload.sName,
    rank: payload.iRank ?? 0,
    reputation: payload.iRep ?? 0
  }), "toFaction");
  var equippedItemId = /* @__PURE__ */ __name((value) => value === void 0 || value === 0 ? void 0 : value, "equippedItemId");
  var maxCombatExitJumps = 3;
  var toOutfit = /* @__PURE__ */ __name((payload) => new LiveOutfit({
    colors: {
      accessory: payload.colors?.accessory,
      base: payload.colors?.base,
      eye: payload.colors?.eye,
      hair: payload.colors?.hair,
      skin: payload.colors?.skin,
      trim: payload.colors?.trim
    },
    equipment: {
      armorItemId: equippedItemId(payload.co),
      capeItemId: equippedItemId(payload.ba),
      classItemId: equippedItemId(payload.ar),
      helmItemId: equippedItemId(payload.he),
      itemId: equippedItemId(payload.None),
      miscItemId: equippedItemId(payload.mi),
      petItemId: equippedItemId(payload.pe),
      weaponItemId: equippedItemId(payload.Weapon)
    },
    name: payload.name
  }), "toOutfit");
  var makePlayer = /* @__PURE__ */ __name((bridge, host4, auth, inventory, map8, wait) => {
    const read = /* @__PURE__ */ __name((method, schema, fallback) => bridge.invoke(method, void 0, schema).pipe(map5(getOrElse(() => fallback))), "read");
    const get5 = /* @__PURE__ */ __name(() => host4.read({ kind: "me" }).pipe(
      map5((player) => player === null ? null : toLivePlayer(player))
    ), "get");
    const readLocation = /* @__PURE__ */ __name((projected, method) => projected !== "" ? succeed7(projected) : map8.isLoaded().pipe(
      flatMap3(
        (loaded) => loaded ? read(method, String4, "") : succeed7("")
      )
    ), "readLocation");
    const getAuras = /* @__PURE__ */ __name((options) => get5().pipe(
      map5(
        (player) => (player?.auras ?? []).filter(
          (aura) => options?.kind === void 0 || aura.kind === options.kind
        )
      )
    ), "getAuras");
    const factionCache = /* @__PURE__ */ new Map();
    const outfitCache = /* @__PURE__ */ new Map();
    const getAura = /* @__PURE__ */ __name((name, options) => getAuras(options).pipe(
      map5(
        (auras2) => auras2.find((aura) => sameText(aura.name, name)) ?? null
      )
    ), "getAura");
    const hasAura = /* @__PURE__ */ __name((name, options) => getAura(name, options).pipe(map5((aura) => aura !== null)), "hasAura");
    const auras = { get: getAura, getAll: getAuras, has: hasAura };
    const getFactions = /* @__PURE__ */ __name(() => bridge.invoke("player.getFactions", void 0, FactionPayloads).pipe(
      map5(
        match({
          onNone: /* @__PURE__ */ __name(() => Array.from(factionCache.values()), "onNone"),
          onSome: /* @__PURE__ */ __name((payloads) => {
            const incoming = payloads.map(toFaction);
            const ids = new Set(incoming.map((faction) => faction.id));
            for (const id of factionCache.keys()) {
              if (!ids.has(id)) factionCache.delete(id);
            }
            for (const faction of incoming) {
              const current = factionCache.get(faction.id);
              if (current !== void 0) current.replaceFrom(faction);
              else factionCache.set(faction.id, faction);
            }
            return Array.from(factionCache.values());
          }, "onSome")
        })
      )
    ), "getFactions");
    const getFaction = /* @__PURE__ */ __name((selector) => getFactions().pipe(
      map5(
        (factions2) => factions2.find(
          (faction) => typeof selector === "number" ? faction.id === selector : sameText(faction.name, selector)
        ) ?? null
      )
    ), "getFaction");
    const factions = { get: getFaction, getAll: getFactions };
    const getOutfits = /* @__PURE__ */ __name(() => bridge.invoke("outfits.getAll", void 0, OutfitPayloads).pipe(
      map5(
        match({
          onNone: /* @__PURE__ */ __name(() => Array.from(outfitCache.values()), "onNone"),
          onSome: /* @__PURE__ */ __name((payloads) => {
            const incoming = payloads.map(toOutfit);
            const keys = new Set(
              incoming.map((outfit) => outfit.name.toLowerCase())
            );
            for (const key of outfitCache.keys()) {
              if (!keys.has(key)) outfitCache.delete(key);
            }
            for (const outfit of incoming) {
              const key = outfit.name.toLowerCase();
              const current = outfitCache.get(key);
              if (current !== void 0) current.replaceFrom(outfit);
              else outfitCache.set(key, outfit);
            }
            return Array.from(outfitCache.values());
          }, "onSome")
        })
      )
    ), "getOutfits");
    const getOutfit = /* @__PURE__ */ __name((name) => getOutfits().pipe(
      map5(
        (outfits2) => outfits2.find((outfit) => sameText(outfit.name, name)) ?? null
      )
    ), "getOutfit");
    const equipOutfit = /* @__PURE__ */ __name((name, keepColors = false) => wait.forGameAction("equipLoadout").pipe(
      flatMap3(
        (ready) => ready ? bridge.invoke("outfits.equip", [name, keepColors], Boolean3).pipe(map5(getOrElse(() => false))) : succeed7(false)
      )
    ), "equipOutfit");
    const wearOutfit = /* @__PURE__ */ __name((name, keepColors = false) => wait.forGameAction("wearLoadout").pipe(
      flatMap3(
        (ready) => ready ? bridge.invoke("outfits.wear", [name, keepColors], Boolean3).pipe(map5(getOrElse(() => false))) : succeed7(false)
      )
    ), "wearOutfit");
    const outfits = {
      equip: equipOutfit,
      get: getOutfit,
      getAll: getOutfits,
      wear: wearOutfit
    };
    const getCell = /* @__PURE__ */ __name(() => get5().pipe(
      flatMap3(
        (current) => readLocation(current?.cell ?? "", "player.getCell")
      )
    ), "getCell");
    const getClassName = /* @__PURE__ */ __name(() => read("player.getClassName", String4, ""), "getClassName");
    const getClassRank = fn2("Player.getClassRank")(function* (query) {
      const inventoryClass = findClassItem(yield* inventory.getAll(), query);
      if (inventoryClass !== void 0) return inventoryClass.classRank;
      if (query === void 0 || !(yield* isHydrated(host4, "bank"))) {
        return null;
      }
      const bankClass = findClassItem(yield* readItems(host4, "bank"), query);
      return bankClass?.classRank ?? null;
    });
    const getGender = /* @__PURE__ */ __name(() => read("player.getGender", String4, ""), "getGender");
    const getGold = /* @__PURE__ */ __name(() => read("player.getGold", WireInt, 0), "getGold");
    const getHp = /* @__PURE__ */ __name(() => get5().pipe(map5((current) => current?.hp ?? 0)), "getHp");
    const getLevel = /* @__PURE__ */ __name(() => get5().pipe(map5((current) => current?.level ?? 0)), "getLevel");
    const getMaxHp = /* @__PURE__ */ __name(() => get5().pipe(map5((current) => current?.maxHp ?? 0)), "getMaxHp");
    const getMaxMp = /* @__PURE__ */ __name(() => get5().pipe(map5((current) => current?.maxMp ?? 0)), "getMaxMp");
    const getMp = /* @__PURE__ */ __name(() => get5().pipe(map5((current) => current?.mp ?? 0)), "getMp");
    const getPad = /* @__PURE__ */ __name(() => get5().pipe(
      flatMap3(
        (current) => readLocation(current?.pad ?? "", "player.getPad")
      )
    ), "getPad");
    const getState = /* @__PURE__ */ __name(() => get5().pipe(map5((current) => current?.state ?? EntityState.Idle)), "getState");
    const getPosition = /* @__PURE__ */ __name(() => get5().pipe(
      map5((current) => ({
        ...current?.position ?? { x: 0, y: 0 }
      }))
    ), "getPosition");
    const goToPlayer = /* @__PURE__ */ __name((name) => {
      const username = name.trim();
      return username === "" ? void_3 : bridge.invoke("player.goToPlayer", [username], Void2).pipe(asVoid2);
    }, "goToPlayer");
    const hasActiveBoost = /* @__PURE__ */ __name((boostType) => bridge.invoke("player.hasActiveBoost", [boostType], Boolean3).pipe(map5(getOrElse(() => false))), "hasActiveBoost");
    const isAfk = /* @__PURE__ */ __name(() => get5().pipe(map5((current) => current?.afk ?? false)), "isAfk");
    const isAlive = /* @__PURE__ */ __name(() => get5().pipe(
      flatMap3(
        (current) => current !== null ? succeed7(current.alive) : all2([
          read("player.getHp", WireInt, 0),
          read("player.getState", WireInt, EntityState.Dead)
        ]).pipe(
          map5(([hp, state]) => hp > 0 && state !== EntityState.Dead)
        )
      )
    ), "isAlive");
    const isMember = /* @__PURE__ */ __name(() => read("player.isMember", Boolean3, false), "isMember");
    const isReady = makePlayerReadiness(host4, auth, map8, bridge);
    const readLiveLocation = /* @__PURE__ */ __name(() => all2({
      cell: bridge.invoke("player.getCell", void 0, String4).pipe(map5(getOrElse(() => ""))),
      pad: bridge.invoke("player.getPad", void 0, String4).pipe(map5(getOrElse(() => "")))
    }), "readLiveLocation");
    const isCellReady = /* @__PURE__ */ __name((cell, pad) => {
      const args2 = pad === void 0 ? [cell] : [cell, pad];
      return bridge.invoke("world.isCellReady", args2, Boolean3).pipe(map5(getOrElse(() => false)));
    }, "isCellReady");
    const jumpToCell = fn2("Player.jumpToCell")(function* (cell, pad) {
      const targetCell = cell.trim();
      if (targetCell === "") return false;
      const args2 = pad === void 0 ? [targetCell] : [targetCell, pad];
      const invoked = yield* bridge.invoke("player.jump", args2, Void2);
      if (isNone2(invoked)) return false;
      let requestedLocationObserved = false;
      let settledAtRequestedLocation = false;
      const settled = yield* wait.until(
        gen2(function* () {
          const observed = yield* readLiveLocation();
          const atRequestedLocation = sameText(observed.cell, targetCell) && (pad === void 0 || sameText(observed.pad, pad));
          if (atRequestedLocation) requestedLocationObserved = true;
          if (!requestedLocationObserved) return false;
          const readinessCell = atRequestedLocation ? targetCell : observed.cell.trim();
          const readinessPad = atRequestedLocation ? pad : observed.pad.trim() || void 0;
          if (readinessCell === "") return false;
          const ready = yield* isCellReady(readinessCell, readinessPad);
          if (ready) settledAtRequestedLocation = atRequestedLocation;
          return ready;
        }),
        { timeout: "3 seconds" }
      );
      if (!settled) return false;
      return settledAtRequestedLocation;
    });
    const stopCombat2 = stopCombat(bridge);
    const waitUntilIdle = /* @__PURE__ */ __name((timeout2) => wait.until(
      getState().pipe(
        flatMap3(
          (state) => state !== EntityState.Idle ? succeed7(false) : sleep2("500 millis").pipe(
            andThen2(getState()),
            map5((confirmed) => confirmed === EntityState.Idle)
          )
        )
      ),
      { interval: "100 millis", timeout: timeout2 }
    ), "waitUntilIdle");
    const exitCombat = fn2("Player.exitCombat")(function* () {
      if ((yield* getState()) === EntityState.Idle) return true;
      const currentCell = yield* getCell();
      const currentPad = yield* getPad();
      const cells = combatExitCells(
        yield* map8.getCells(),
        yield* host4.read({ kind: "monsters" }),
        currentCell
      );
      for (const cell of cells.slice(0, maxCombatExitJumps)) {
        yield* stopCombat2;
        yield* jumpToCell(
          cell,
          sameText(cell, currentCell) ? currentPad : void 0
        );
        if (yield* waitUntilIdle("2 seconds")) return true;
      }
      return false;
    });
    const joinMap = /* @__PURE__ */ __name((target, options) => gen2(function* () {
      const { cell, pad } = options ?? {};
      const destination = yield* parseMapTarget(target);
      if (destination.map === "") return false;
      const targetCell = cell ?? (pad === void 0 ? void 0 : "Enter");
      const currentMap = yield* host4.read({ kind: "map" });
      const alreadyLoaded = (yield* map8.isLoaded()) && sameText(currentMap.name, destination.name) && (!destination.requireExactRoom || currentMap.roomNumber === destination.roomNumber);
      if (alreadyLoaded) {
        if (!(yield* wait.until(isReady(), { timeout: "10 seconds" })))
          return false;
        return targetCell === void 0 ? true : yield* jumpToCell(targetCell, pad);
      }
      if (!(yield* wait.until(isReady(), { timeout: "10 seconds" })))
        return false;
      if (!(yield* wait.forGameAction("tfer", { timeout: "10 seconds" })))
        return false;
      if (!(yield* wait.until(isAlive(), { timeout: "15 seconds" })))
        return false;
      if (!(yield* exitCombat())) return false;
      const args2 = cell === void 0 && pad === void 0 ? [destination.map] : pad === void 0 ? [destination.map, cell ?? "Enter"] : [destination.map, cell ?? "Enter", pad];
      if (isNone2(yield* bridge.invoke("player.joinMap", args2, Void2)))
        return false;
      const loaded = yield* wait.until(
        host4.read({ kind: "map" }).pipe(
          flatMap3(
            (current) => map8.isLoaded().pipe(
              map5(
                (ready) => ready && sameText(current.name, destination.name) && (!destination.requireExactRoom || current.roomNumber === destination.roomNumber)
              )
            )
          )
        ),
        { timeout: "20 seconds" }
      );
      if (!loaded) return false;
      if (!(yield* wait.until(isReady(), { timeout: "10 seconds" })))
        return false;
      if (targetCell === void 0) return true;
      return yield* jumpToCell(targetCell, pad);
    }), "joinMap");
    const rest = /* @__PURE__ */ __name((options) => gen2(function* () {
      const current = yield* get5();
      if (current !== null && current.hp >= current.maxHp && current.mp >= current.maxMp)
        return true;
      if (!(yield* wait.forGameAction("rest"))) return false;
      if (isNone2(
        yield* bridge.invoke("player.rest", void 0, Void2)
      ))
        return false;
      if (options?.waitUntilFull !== true) return true;
      return yield* wait.until(
        get5().pipe(
          map5(
            (player) => player !== null && player.hp >= player.maxHp && player.mp >= player.maxMp
          )
        ),
        { timeout: "10 seconds" }
      );
    }), "rest");
    const walkTo = /* @__PURE__ */ __name((position, options) => gen2(function* () {
      if (!(yield* isAlive())) return false;
      const targetX = Math.trunc(position.x);
      const targetY = Math.trunc(position.y);
      const speed = options?.speed;
      const args2 = speed === void 0 ? [targetX, targetY] : [targetX, targetY, speed];
      const started = yield* bridge.invoke("player.walkTo", args2, Boolean3).pipe(map5(getOrElse(() => false)));
      if (!started) return false;
      return yield* wait.until(
        get5().pipe(
          map5(
            (current) => current?.position.x === targetX && current.position.y === targetY
          )
        ),
        { timeout: "3 seconds" }
      );
    }), "walkTo");
    return {
      auras,
      exitCombat,
      factions,
      get: get5,
      getCell,
      getClassName,
      getClassRank,
      getGender,
      getGold,
      getHp,
      getLevel,
      getMaxHp,
      getMaxMp,
      getMp,
      getPad,
      getPosition,
      getState,
      goToPlayer,
      hasActiveBoost,
      isAfk,
      isAlive,
      isMember,
      isReady,
      joinMap,
      jumpToCell,
      outfits,
      rest,
      walkTo
    };
  }, "makePlayer");
  var makePlayerReadiness = /* @__PURE__ */ __name((host4, auth, map8, bridge) => () => host4.read({ kind: "readiness" }).pipe(
    flatMap3(
      (readiness) => readiness.missing.length === 0 ? all2([
        auth.isLoggedIn(),
        map8.isLoaded(),
        bridge.invoke("player.isLoaded", void 0, Boolean3).pipe(map5(getOrElse(() => false)))
      ]).pipe(
        map5(
          ([loggedIn, loaded, playerLoaded]) => loggedIn && loaded && playerLoaded
        )
      ) : succeed7(false)
    )
  ), "makePlayerReadiness");

  // packages/runtime/src/game/flash/api/Players.ts
  var makePlayers = /* @__PURE__ */ __name((host4) => {
    const get5 = /* @__PURE__ */ __name((selector) => isSelectable(selector) ? host4.read({ kind: "player", selector }).pipe(
      map5(
        (player) => player === null ? null : toLivePlayer(player)
      )
    ) : succeed7(null), "get");
    const getAll = /* @__PURE__ */ __name(() => host4.read({ kind: "players" }).pipe(map5((players) => players.map(toLivePlayer))), "getAll");
    const getMe = /* @__PURE__ */ __name(() => host4.read({ kind: "me" }).pipe(
      map5((player) => player === null ? null : toLivePlayer(player))
    ), "getMe");
    return {
      get: get5,
      getAll,
      getMe
    };
  }, "makePlayers");

  // packages/runtime/src/game/flash/api/Quests.ts
  var isQuestId = /* @__PURE__ */ __name((questId) => Number.isSafeInteger(questId) && questId > 0, "isQuestId");
  var makeQuests = /* @__PURE__ */ __name((bridge, host4, wait) => {
    const loads = makeUnsafe5(1);
    let nextLoadAt = 0;
    const read = /* @__PURE__ */ __name((questId) => host4.read({ kind: "quest", id: questId }).pipe(
      map5((quest) => quest === null ? null : toLiveQuest(quest))
    ), "read");
    const isLoaded = /* @__PURE__ */ __name((questId) => host4.read({ kind: "quest", id: questId }).pipe(map5((quest) => quest !== null)), "isLoaded");
    const waitForLoadSlot = gen2(function* () {
      const delay3 = nextLoadAt - (yield* currentTimeMillis2);
      if (delay3 > 0) yield* sleep2(delay3);
      nextLoadAt = (yield* currentTimeMillis2) + 1500;
    });
    const requestBatch = fn2("Quests.requestBatch")(
      function* (questIds, silent) {
        const missing2 = yield* filter2(
          questIds,
          (id) => isLoaded(id).pipe(map5((loaded) => !loaded))
        );
        if (missing2.length === 0) return true;
        yield* waitForLoadSlot;
        yield* bridge.invoke(
          silent ? "quests.getMultiple" : "quests.loadMultiple",
          [missing2.join(",")],
          Void2
        ).pipe(flatMap3(fromOption3));
        return yield* wait.until(
          forEach2(missing2, isLoaded).pipe(
            map5((results2) => results2.every(Boolean))
          ),
          { timeout: "5 seconds" }
        );
      },
      catch_2(() => succeed7(false)),
      repeat2({ times: 1, until: Boolean })
    );
    const loadBatch = fn2("Quests.loadBatch")(function* (questIds, silent = false) {
      const ids = dedupe(questIds.filter(isQuestId));
      const initial = yield* forEach2(ids, isLoaded);
      if (initial.every(Boolean)) return initial;
      yield* forEach2(
        chunksOf(ids, 30),
        (batch) => requestBatch(batch, silent),
        { concurrency: 1, discard: true }
      ).pipe(loads.withPermit);
      return yield* forEach2(ids, isLoaded);
    });
    const load = /* @__PURE__ */ __name((questId, silent = false) => loadBatch([questId], silent).pipe(
      map5((loaded) => loaded[0] ?? false)
    ), "load");
    const isInProgress = /* @__PURE__ */ __name((questId) => {
      if (!isQuestId(questId)) return succeed7(false);
      return bridge.invoke("quests.isInProgress", [questId], Boolean3).pipe(map5(getOrElse(() => false)));
    }, "isInProgress");
    const canComplete = /* @__PURE__ */ __name((questId) => {
      if (!isQuestId(questId)) return succeed7(false);
      return bridge.invoke("quests.canComplete", [questId], Boolean3).pipe(map5(getOrElse(() => false)));
    }, "canComplete");
    const accept = /* @__PURE__ */ __name((questId, silent = false) => {
      if (!isQuestId(questId)) return succeed7(false);
      return gen2(function* () {
        let actionAvailable = false;
        if (!(yield* isLoaded(questId))) {
          actionAvailable = yield* wait.forGameAction("acceptQuest");
          if (!actionAvailable || !(yield* load(questId, silent))) return false;
        }
        if (yield* isInProgress(questId)) return true;
        if (!actionAvailable && !(yield* wait.forGameAction("acceptQuest")))
          return false;
        const accepted = yield* bridge.invoke("quests.accept", [questId, silent], Boolean3).pipe(map5(getOrElse(() => false)));
        return accepted ? yield* wait.until(isInProgress(questId), {
          timeout: "5 seconds"
        }) : false;
      });
    }, "accept");
    const abandon = /* @__PURE__ */ __name((questId) => {
      if (!isQuestId(questId)) return succeed7(false);
      return gen2(function* () {
        if (!(yield* isInProgress(questId))) return false;
        if (isNone2(
          yield* bridge.invoke("quests.abandon", [questId], Void2)
        )) {
          return false;
        }
        return yield* wait.until(
          isInProgress(questId).pipe(map5((active) => !active)),
          { timeout: "5 seconds" }
        );
      });
    }, "abandon");
    const acceptBatch = /* @__PURE__ */ __name((questIds, silent = false) => forEach2(questIds, (questId) => accept(questId, silent), {
      concurrency: 1
    }), "acceptBatch");
    const getMaxTurnIns = /* @__PURE__ */ __name((questId) => {
      if (!isQuestId(questId)) return succeed7(1);
      return bridge.invoke("quests.getMaxTurnIns", [questId], WireInt).pipe(
        map5(
          match({
            onNone: /* @__PURE__ */ __name(() => 1, "onNone"),
            onSome: /* @__PURE__ */ __name((turnIns) => Math.max(1, turnIns), "onSome")
          })
        )
      );
    }, "getMaxTurnIns");
    const complete = /* @__PURE__ */ __name((questId, options) => {
      if (!isQuestId(questId)) return succeed7(false);
      if (options?.turnIns !== void 0 && !Number.isFinite(options.turnIns)) {
        return succeed7(false);
      }
      return gen2(function* () {
        if (!(yield* isInProgress(questId))) return false;
        if (!(yield* canComplete(questId))) return false;
        if (!(yield* wait.forGameAction("tryQuestComplete", {
          timeout: "5 seconds"
        }))) {
          return false;
        }
        const turnIns = options?.turnIns === void 0 ? yield* getMaxTurnIns(questId) : Math.max(1, Math.trunc(options.turnIns));
        const event = yield* wait.forEvent(
          { questId, type: "quest-complete" },
          {
            timeout: "5 seconds",
            trigger: bridge.invoke(
              "quests.complete",
              [questId, turnIns, options?.rewardItemId ?? -1, false],
              Void2
            ).pipe(map5(isSome2))
          }
        );
        return event !== null;
      });
    }, "complete");
    const get5 = /* @__PURE__ */ __name((questId) => {
      if (!isQuestId(questId)) return succeed7(null);
      return read(questId).pipe(
        flatMap3(
          (quest) => quest !== null ? succeed7(quest) : load(questId).pipe(andThen2(read(questId)))
        )
      );
    }, "get");
    const getAccepted = /* @__PURE__ */ __name(() => host4.command({ kind: "refreshAcceptedQuests" }).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => host4.read({ kind: "acceptedQuests" }), "onNone"),
          onSome: succeed7
        })
      ),
      map5((quests) => quests.map(toLiveQuest))
    ), "getAccepted");
    const getAll = /* @__PURE__ */ __name(() => host4.read({ kind: "quests" }).pipe(map5((quests) => quests.map(toLiveQuest))), "getAll");
    const isAvailable = /* @__PURE__ */ __name((questId) => {
      if (!isQuestId(questId)) return succeed7(false);
      return bridge.invoke("quests.isAvailable", [questId], Boolean3).pipe(map5(getOrElse(() => false)));
    }, "isAvailable");
    return {
      abandon,
      accept,
      acceptBatch,
      canComplete,
      complete,
      get: get5,
      getAccepted,
      getAll,
      getMaxTurnIns,
      isAvailable,
      isInProgress,
      load,
      loadBatch
    };
  }, "makeQuests");

  // packages/runtime/src/game/flash/api/ScriptReactions.ts
  var start = fnUntraced2(function* ({
    antiCounter,
    events,
    gateway,
    host: host4,
    monsters
  }) {
    const ensureCurrentCell = /* @__PURE__ */ __name(() => monsters.lookup.getAll().pipe(flatMap3(monsters.ensureCurrentCell)), "ensureCurrentCell");
    yield* events.on({ type: "join-map" }, ensureCurrentCell);
    yield* events.on(
      { type: "player-location" },
      (event) => host4.read({ kind: "me" }).pipe(
        flatMap3(
          (player) => player?.entityId === event.entityId ? ensureCurrentCell() : void_3
        )
      )
    );
    yield* gateway.start(antiCounter.handleEvent);
  });
  var ScriptReactions = class extends Service()(
    "lucent/renderer/flash/ScriptReactions"
  ) {
    static {
      __name(this, "ScriptReactions");
    }
  };
  var layer4 = effectDiscard(flatMap3(ScriptReactions, start));

  // packages/runtime/src/game/flash/api/Settings.ts
  var makeSettings = /* @__PURE__ */ __name((host4) => {
    const get5 = /* @__PURE__ */ __name(() => host4.read({ kind: "settings" }), "get");
    return {
      apply: /* @__PURE__ */ __name((patch) => gen2(function* () {
        for (const field2 of ["frameRate", "walkSpeed"]) {
          if (typeof patch[field2] === "number" && !Number.isFinite(patch[field2])) {
            return yield* die3(new Error(`${field2} must be a finite number`));
          }
        }
        const result2 = yield* host4.command({ kind: "updateSettings", patch });
        if (isNone2(result2)) {
          return yield* die3(new Error("host rejected settings patch"));
        }
      }), "apply"),
      get: get5,
      isAntiCounterEnabled: /* @__PURE__ */ __name(() => get5().pipe(map5((state) => state.antiCounterEnabled)), "isAntiCounterEnabled")
    };
  }, "makeSettings");

  // packages/runtime/src/game/flash/api/Shops.ts
  var BuyResponse = Struct({
    bBank: optionalKey2(WireBoolean),
    bitSuccess: WireBoolean,
    iQty: optionalKey2(WireInt)
  });
  var ShopIdentity = Struct({ ShopID: PositiveWireInt });
  var ShopLoadResponse = Union2([
    ShopIdentity,
    Struct({ shopinfo: ShopIdentity })
  ]);
  var decodeBuyResponse = decodeUnknownOption2(BuyResponse);
  var decodeShopLoadResponse = decodeUnknownOption2(ShopLoadResponse);
  var isShopId = /* @__PURE__ */ __name((shopId) => Number.isSafeInteger(shopId) && shopId > 0, "isShopId");
  var selectorFor = /* @__PURE__ */ __name((item) => item.shopItemId === void 0 ? { itemId: item.itemId } : { shopItemId: item.shopItemId }, "selectorFor");
  var makeShops = /* @__PURE__ */ __name((bridge, host4, inventory, wait) => {
    const get5 = /* @__PURE__ */ __name((selector) => readItem(host4, "shop", selector), "get");
    const getCurrent = /* @__PURE__ */ __name(() => host4.read({ kind: "shop" }).pipe(map5((shop) => shop === null ? null : toLiveShop(shop))), "getCurrent");
    const isOpen = /* @__PURE__ */ __name((shopId) => {
      if (shopId !== void 0 && !isShopId(shopId)) {
        return succeed7(false);
      }
      return bridge.invoke(
        "shops.isOpen",
        shopId === void 0 ? void 0 : [shopId],
        Boolean3
      ).pipe(
        flatMap3(
          match({
            onNone: /* @__PURE__ */ __name(() => host4.read({ kind: "shop" }).pipe(
              map5(
                (shop) => shopId !== void 0 ? shop?.id === shopId : shop !== null
              )
            ), "onNone"),
            onSome: /* @__PURE__ */ __name((open) => succeed7(open), "onSome")
          })
        )
      );
    }, "isOpen");
    function close2(shopId) {
      if (shopId !== void 0 && !isShopId(shopId)) {
        return succeed7(false);
      }
      return gen2(function* () {
        if (!(yield* isOpen(shopId))) return false;
        const closed = yield* bridge.invoke(
          "shops.close",
          shopId === void 0 ? void 0 : [shopId],
          Boolean3
        ).pipe(map5(getOrElse(() => false)));
        if (!closed) return false;
        const settled = yield* wait.until(
          isOpen(shopId).pipe(map5((open) => !open)),
          { timeout: "3 seconds" }
        );
        if (!settled) return false;
        return getOrElse(
          yield* host4.command({
            kind: "confirmShopClosed",
            shopId: shopId ?? null
          }),
          () => false
        );
      });
    }
    __name(close2, "close");
    const load = /* @__PURE__ */ __name((shopId) => {
      if (!isShopId(shopId)) return succeed7(false);
      return gen2(function* () {
        if (yield* isOpen(shopId)) return true;
        const current = yield* host4.read({ kind: "shop" });
        const reopensCachedShop = current?.id === shopId && current.limited === false;
        if (current !== null && current.id !== shopId && (yield* isOpen(current.id))) {
          yield* close2(current.id);
        }
        if (!(yield* wait.forGameAction("loadShop", { timeout: "5 seconds" }))) {
          return false;
        }
        if (reopensCachedShop) {
          if (isNone2(
            yield* bridge.invoke("shops.load", [shopId], Void2)
          )) {
            return false;
          }
          return yield* wait.until(isOpen(shopId), { timeout: "5 seconds" });
        }
        const packet = yield* wait.forPacket(
          {
            command: "loadShop",
            direction: "extension",
            predicate: /* @__PURE__ */ __name((candidate) => {
              const decoded = decodeShopLoadResponse(packetData(candidate));
              if (isNone2(decoded)) return false;
              const identity2 = "shopinfo" in decoded.value ? decoded.value.shopinfo : decoded.value;
              return identity2.ShopID === shopId;
            }, "predicate"),
            encoding: "json"
          },
          {
            timeout: "5 seconds",
            trigger: bridge.invoke("shops.load", [shopId], Void2).pipe(map5(isSome2))
          }
        );
        if (packet === null) return false;
        const [shop, open] = yield* all2([
          host4.read({ kind: "shop" }),
          isOpen(shopId)
        ]);
        return shop?.id === shopId && open;
      });
    }, "load");
    const canBuy = /* @__PURE__ */ __name((selector, options) => get5(selector).pipe(
      flatMap3(
        (item) => item === null ? succeed7(false) : bridge.invoke(
          "shops.canBuyItem",
          [selectorFor(item), normalizeItemQuantity(options?.quantity)],
          Boolean3
        ).pipe(map5(getOrElse(() => false)))
      )
    ), "canBuy");
    const buy = /* @__PURE__ */ __name((selector, options) => get5(selector).pipe(
      flatMap3((item) => {
        if (item === null) return succeed7(false);
        const requested = normalizeItemQuantity(options?.quantity);
        return gen2(function* () {
          if (!(yield* wait.forGameAction("buyItem"))) return false;
          if (!(yield* canBuy(selectorFor(item), { quantity: requested }))) {
            return false;
          }
          const startingQuantities = yield* all2({
            bank: itemQuantity(host4, "bank", item.itemId),
            house: itemQuantity(host4, "house", item.itemId),
            inventory: itemQuantity(host4, "inventory", item.itemId)
          });
          let response;
          const packet = yield* wait.forPacket(
            {
              command: "buyItem",
              direction: "extension",
              predicate: /* @__PURE__ */ __name((candidate) => {
                const decoded = decodeBuyResponse(packetData(candidate));
                if (isNone2(decoded)) return false;
                response = decoded.value;
                return true;
              }, "predicate"),
              encoding: "json"
            },
            {
              timeout: "5 seconds",
              trigger: bridge.invoke(
                "shops.buy",
                [selectorFor(item), requested],
                Void2
              ).pipe(map5(isSome2))
            }
          );
          if (packet === null || response?.bitSuccess !== true) return false;
          const container2 = response.bBank && item.category !== "Class" ? "bank" : item.houseItem ? "house" : "inventory";
          yield* host4.command({ kind: "refreshContainer", container: container2 });
          const owned = yield* itemQuantity(host4, container2, item.itemId);
          return owned >= startingQuantities[container2] + requested;
        });
      })
    ), "buy");
    const sell = /* @__PURE__ */ __name((selector, options) => {
      return inventory.get(selector).pipe(
        flatMap3((item) => {
          if (item === null) return succeed7(false);
          const requested = normalizeItemQuantity(options?.quantity);
          if (item.quantity < requested) return succeed7(false);
          return gen2(function* () {
            if (!(yield* wait.forGameAction("sellItem"))) return false;
            const sold = yield* bridge.invoke(
              "shops.sell",
              [toItemSelector(selector), requested],
              Boolean3
            ).pipe(map5(getOrElse(() => false)));
            if (!sold) return false;
            return yield* wait.until(
              inventory.get(item.itemId).pipe(
                map5(
                  (current) => current === null || current.quantity <= item.quantity - requested
                )
              ),
              { timeout: "5 seconds" }
            );
          });
        })
      );
    }, "sell");
    const getAll = /* @__PURE__ */ __name(() => readItems(host4, "shop"), "getAll");
    const getMaxBuyQuantity = /* @__PURE__ */ __name((selector) => get5(selector).pipe(
      flatMap3(
        (item) => item === null ? succeed7(0) : bridge.invoke("shops.getMaxBuyQuantity", [selectorFor(item)], WireInt).pipe(map5(getOrElse(() => 0)))
      )
    ), "getMaxBuyQuantity");
    const isMergeShop = /* @__PURE__ */ __name(() => bridge.invoke("shops.isMergeShop", void 0, Boolean3).pipe(
      flatMap3(
        match({
          onNone: /* @__PURE__ */ __name(() => host4.read({ kind: "shop" }).pipe(map5((shop) => shop?.merge ?? false)), "onNone"),
          onSome: /* @__PURE__ */ __name((merge3) => succeed7(merge3), "onSome")
        })
      )
    ), "isMergeShop");
    const openArmorCustomize = /* @__PURE__ */ __name(() => bridge.invoke("shops.loadArmorCustomize", void 0, Void2).pipe(asVoid2), "openArmorCustomize");
    const openHairShop = /* @__PURE__ */ __name((shopId) => {
      if (!isShopId(shopId)) return void_3;
      return wait.forGameAction("loadHairShop", { timeout: "5 seconds" }).pipe(
        flatMap3(
          (ready) => ready ? bridge.invoke("shops.loadHairShop", [shopId], Void2) : succeed7(none2())
        ),
        asVoid2
      );
    }, "openHairShop");
    return {
      buy,
      canBuy,
      close: close2,
      get: get5,
      getAll,
      getCurrent,
      getMaxBuyQuantity,
      isMergeShop,
      isOpen,
      load,
      openArmorCustomize,
      openHairShop,
      sell
    };
  }, "makeShops");

  // packages/runtime/src/game/flash/api/TempInventory.ts
  var makeTempInventory = /* @__PURE__ */ __name((host4) => {
    const contains = /* @__PURE__ */ __name((selector, quantity) => itemQuantity(host4, "temporary", selector).pipe(
      map5((owned) => owned >= normalizeItemQuantity(quantity))
    ), "contains");
    const get5 = /* @__PURE__ */ __name((selector) => readItem(host4, "temporary", selector), "get");
    const getAll = /* @__PURE__ */ __name(() => readItems(host4, "temporary"), "getAll");
    return {
      contains,
      get: get5,
      getAll
    };
  }, "makeTempInventory");

  // packages/runtime/src/game/flash/protocol/Wait.ts
  var takeMatching = /* @__PURE__ */ __name((subscription, matches) => gen2(function* () {
    while (true) {
      const value = yield* take2(subscription);
      if (matches(value)) return value;
    }
  }), "takeMatching");
  var withTimeout = /* @__PURE__ */ __name((effect2, timeout2) => timeout2 === void 0 ? effect2.pipe(map5((value) => value)) : effect2.pipe(timeoutOption2(timeout2), map5(getOrNull)), "withTimeout");
  var makeWait = /* @__PURE__ */ __name((source) => ({
    forEvent: /* @__PURE__ */ __name(((selector, options) => scoped2(
      gen2(function* () {
        const subscription = yield* source.subscribeEvents;
        if (options?.trigger !== void 0) {
          const responseExpected = yield* options.trigger;
          if (!responseExpected) return null;
        }
        return yield* withTimeout(
          takeMatching(
            subscription,
            (event) => matchesEvent(event, selector)
          ),
          options?.timeout
        );
      })
    )), "forEvent"),
    forProjectionEvent: /* @__PURE__ */ __name(((selector, options) => scoped2(
      gen2(function* () {
        const subscription = yield* source.subscribeEvents;
        if (options?.trigger !== void 0) {
          const responseExpected = yield* options.trigger;
          if (!responseExpected) return null;
        }
        return yield* withTimeout(
          takeMatching(
            subscription,
            (event) => isProjectionEvent(event) && matchesEvent(event, selector)
          ),
          options?.timeout
        );
      })
    )), "forProjectionEvent"),
    forPacket: /* @__PURE__ */ __name(((selector, options) => scoped2(
      gen2(function* () {
        const subscription = yield* source.subscribePackets;
        if (options?.trigger !== void 0) {
          const responseExpected = yield* options.trigger;
          if (!responseExpected) return null;
        }
        return yield* withTimeout(
          takeMatching(
            subscription,
            (packet) => matchesPacket(packet, selector)
          ),
          options?.timeout
        );
      })
    )), "forPacket"),
    until: /* @__PURE__ */ __name((condition, options) => {
      const awaited = repeat2(condition, {
        schedule: spaced(options?.interval ?? "100 millis"),
        until: Boolean
      }).pipe(as2(true));
      return options?.timeout === void 0 ? awaited : awaited.pipe(
        timeoutOption2(options.timeout),
        map5(isSome2)
      );
    }, "until"),
    untilSome: /* @__PURE__ */ __name((condition, options) => {
      const awaited = repeat2(condition, {
        schedule: spaced(options?.interval ?? "100 millis"),
        until: isSome2
      });
      return options?.timeout === void 0 ? awaited.pipe(map5(getOrNull)) : awaited.pipe(
        timeoutOption2(options.timeout),
        map5(flatten),
        map5(getOrNull)
      );
    }, "untilSome")
  }), "makeWait");

  // packages/runtime/src/game/flash/api/Wait.ts
  var isWaitOptions = /* @__PURE__ */ __name((options) => typeof options === "object" && !Array.isArray(options) && ("timeout" in options || "interval" in options), "isWaitOptions");
  var makeWaitApi = /* @__PURE__ */ __name((bridge, gateway) => {
    const wait = makeWait(gateway);
    const isGameActionAvailable = /* @__PURE__ */ __name((action) => bridge.invoke("world.isActionAvailable", [action], Boolean3).pipe(map5(getOrElse(() => false))), "isGameActionAvailable");
    const forGameAction = /* @__PURE__ */ __name((action, options) => {
      const normalized = options === void 0 ? {} : isWaitOptions(options) ? options : { timeout: options };
      return wait.until(isGameActionAvailable(action), {
        ...normalized,
        timeout: normalized.timeout ?? "2 seconds"
      });
    }, "forGameAction");
    return {
      ...wait,
      forGameAction,
      isGameActionAvailable
    };
  }, "makeWaitApi");

  // packages/runtime/src/game/flash/api/Api.ts
  var makeApiServices = gen2(function* () {
    const bridge = yield* Bridge;
    const gateway = yield* Gateway;
    const host4 = yield* HostState;
    const wait = makeWaitApi(bridge, gateway);
    const events = yield* makeEvents(gateway, wait);
    const packet = yield* makePacket(gateway, host4, wait);
    const auth = makeAuth(bridge, host4, wait);
    const house = makeHouse(bridge, host4);
    const inventory = makeInventory(bridge, host4, house, wait);
    const bank = yield* makeBank(bridge, host4, auth, inventory, house, wait);
    const drops = yield* makeDrops(bridge, host4, auth);
    const map8 = makeMap(bridge, host4, wait);
    const monsterServices = yield* makeMonsters(bridge, host4, wait);
    const monsters = monsterServices.api;
    const players = makePlayers(host4);
    const player = makePlayer(bridge, host4, auth, inventory, map8, wait);
    const quests = makeQuests(bridge, host4, wait);
    const settings = makeSettings(host4);
    const antiCounter = makeAntiCounter(bridge, settings.isAntiCounterEnabled);
    const shops = makeShops(bridge, host4, inventory, wait);
    const tempInventory = makeTempInventory(host4);
    const combat = makeCombat(
      bridge,
      antiCounter,
      host4,
      drops,
      events,
      inventory,
      map8,
      monsterServices.lookup,
      player,
      players,
      settings,
      tempInventory,
      wait
    );
    return {
      api: {
        auth,
        bank,
        combat,
        drops,
        events,
        house,
        inventory,
        map: map8,
        monsterLookup: monsterServices.lookup,
        monsters,
        packet,
        player,
        players,
        quests,
        settings,
        shops,
        tempInventory,
        wait
      },
      scriptReactions: {
        antiCounter,
        events,
        gateway,
        host: host4,
        monsters: monsterServices
      }
    };
  });
  var makeApi = makeApiServices.pipe(
    tap2(({ scriptReactions }) => start(scriptReactions)),
    map5(({ api: api2 }) => api2)
  );
  var Api = class extends Service()(
    "lucent/renderer/flash/Api"
  ) {
    static {
      __name(this, "Api");
    }
  };
  var layer5 = effectContext(
    makeApiServices.pipe(
      map5(
        ({ api: api2, scriptReactions }) => make2(Api, api2).pipe(
          add(ScriptReactions, scriptReactions)
        )
      )
    )
  );

  // packages/runtime/src/game/flash/apiLayer.ts
  var gatewayLayer = layer2.pipe(provideMerge(layer));
  var hostLayer = layer3.pipe(provideMerge(gatewayLayer));
  var servicesLayer = layer5.pipe(provideMerge(hostLayer));
  var scriptApiLayer = layer4.pipe(
    provideMerge(servicesLayer)
  );

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/Ref.js
  var TypeId20 = "~effect/Ref";
  var RefProto = {
    [TypeId20]: {
      _A: identity
    },
    ...PipeInspectableProto,
    toJSON() {
      return {
        _id: "Ref",
        ref: this.ref
      };
    }
  };
  var makeUnsafe6 = /* @__PURE__ */ __name((value) => {
    const self = Object.create(RefProto);
    self.ref = make12(value);
    return self;
  }, "makeUnsafe");
  var make16 = /* @__PURE__ */ __name((value) => sync2(() => makeUnsafe6(value)), "make");
  var get3 = /* @__PURE__ */ __name((self) => sync2(() => self.ref.current), "get");
  var set2 = /* @__PURE__ */ dual(2, (self, value) => sync2(() => set(self.ref, value)));
  var update = /* @__PURE__ */ dual(2, (self, f) => sync2(() => {
    self.ref.current = f(self.ref.current);
  }));
  var updateAndGet = /* @__PURE__ */ dual(2, (self, f) => sync2(() => self.ref.current = f(self.ref.current)));

  // node_modules/.pnpm/effect@4.0.3/node_modules/effect/dist/SubscriptionRef.js
  var TypeId21 = "~effect/SubscriptionRef";
  var Proto2 = {
    ...PipeInspectableProto,
    [TypeId21]: {
      _A: identity
    },
    toJSON() {
      return {
        _id: "SubscriptionRef",
        value: this.value
      };
    }
  };
  var make17 = /* @__PURE__ */ __name((value) => map5(unbounded({
    replay: 1
  }), (pubsub) => {
    const self = Object.create(Proto2);
    self.semaphore = makeUnsafe5(1);
    self.value = value;
    self.pubsub = pubsub;
    publishUnsafe(self.pubsub, value);
    return self;
  }), "make");
  var changes = /* @__PURE__ */ __name((self) => fromPubSub(self.pubsub), "changes");
  var get4 = /* @__PURE__ */ __name((self) => sync2(() => self.value), "get");
  var setUnsafe = /* @__PURE__ */ __name((self, value) => {
    self.value = value;
    publishUnsafe(self.pubsub, value);
  }, "setUnsafe");
  var set3 = /* @__PURE__ */ dual(2, (self, value) => self.semaphore.withPermit(sync2(() => setUnsafe(self, value))));

  // packages/runtime/src/game/audio/beep.ts
  var SOURCE_STOP_PADDING = 0.05;
  var CLEANUP_AFTER_MS = 1050;
  var REPEAT_INTERVAL = 0.28;
  var SILENCE = 1e-4;
  var sharedContext = null;
  var getAudioContext = /* @__PURE__ */ __name(() => {
    if (sharedContext) return sharedContext;
    try {
      sharedContext = new AudioContext();
      return sharedContext;
    } catch {
      return null;
    }
  }, "getAudioContext");
  var renderSuccessBeep = /* @__PURE__ */ __name((context3, times) => {
    const now = context3.currentTime;
    const master = context3.createGain();
    master.gain.value = 0.5;
    master.connect(context3.destination);
    const delay3 = context3.createDelay(1);
    delay3.delayTime.value = 0.1;
    const feedbackFilter = context3.createBiquadFilter();
    feedbackFilter.type = "lowpass";
    feedbackFilter.frequency.value = 4500;
    const feedbackGain = context3.createGain();
    feedbackGain.gain.value = 0.22;
    const wetGain = context3.createGain();
    wetGain.gain.value = 0.16;
    master.connect(delay3);
    delay3.connect(feedbackFilter);
    feedbackFilter.connect(feedbackGain);
    feedbackGain.connect(delay3);
    feedbackFilter.connect(wetGain);
    wetGain.connect(context3.destination);
    const notes = [
      { frequency: 880, offset: 0, attack: 4e-3, decay: 0.09, peak: 0.06 },
      {
        frequency: 1108.73,
        offset: 0.06,
        attack: 4e-3,
        decay: 0.1,
        peak: 0.06
      },
      {
        frequency: 1318.51,
        offset: 0.12,
        attack: 4e-3,
        decay: 0.18,
        peak: 0.07
      }
    ];
    for (let repetition = 0; repetition < times; repetition += 1) {
      for (const note of notes) {
        const start3 = now + repetition * REPEAT_INTERVAL + note.offset;
        const end2 = start3 + note.attack + note.decay;
        const oscillator = context3.createOscillator();
        const envelope = context3.createGain();
        oscillator.type = "sine";
        oscillator.frequency.setValueAtTime(note.frequency, start3);
        envelope.gain.setValueAtTime(SILENCE, start3);
        envelope.gain.exponentialRampToValueAtTime(
          note.peak,
          start3 + note.attack
        );
        envelope.gain.exponentialRampToValueAtTime(SILENCE, end2);
        oscillator.connect(envelope).connect(master);
        oscillator.start(start3);
        oscillator.stop(end2 + SOURCE_STOP_PADDING);
      }
    }
    window.setTimeout(
      () => {
        master.disconnect();
        delay3.disconnect();
        feedbackFilter.disconnect();
        feedbackGain.disconnect();
        wetGain.disconnect();
      },
      CLEANUP_AFTER_MS + (times - 1) * REPEAT_INTERVAL * 1e3
    );
  }, "renderSuccessBeep");
  var playBeep = /* @__PURE__ */ __name((times) => {
    const context3 = getAudioContext();
    if (!context3) return;
    if (context3.state === "running") {
      renderSuccessBeep(context3, times);
      return;
    }
    void context3.resume().then(
      () => {
        if (context3.state === "running") renderSuccessBeep(context3, times);
      },
      () => void 0
    );
  }, "playBeep");

  // packages/runtime/src/game/scripting/ScriptDialogs.ts
  var ScriptDialogs = class extends Service()("lucent/game/scripting/ScriptDialogs") {
    static {
      __name(this, "ScriptDialogs");
    }
  };
  var snapshotRequest = /* @__PURE__ */ __name((request) => ({ ...request }), "snapshotRequest");
  var snapshotCurrent = /* @__PURE__ */ __name((request) => request === null ? null : snapshotRequest(request), "snapshotCurrent");
  var responseMatchesRequest = /* @__PURE__ */ __name((request, response) => request.id === response.id && request.kind === response.kind, "responseMatchesRequest");
  var requestWithId = /* @__PURE__ */ __name((request, id) => {
    switch (request.kind) {
      case "alert":
        return { ...request, id };
      case "confirm":
        return { ...request, id };
      case "prompt":
        return { ...request, id };
    }
  }, "requestWithId");
  var makeScriptDialogs = fnUntraced2(function* () {
    const scope3 = yield* scope2;
    const runFork3 = runForkWith2(yield* context2());
    const queue = yield* unbounded2();
    const activeRef = yield* make16(null);
    const currentRef = yield* make17(
      null
    );
    const nextIdRef = yield* make16(0);
    const clearActive = /* @__PURE__ */ __name((pending2) => gen2(function* () {
      const active = yield* get3(activeRef);
      if (active !== pending2) return;
      yield* set2(activeRef, null);
      yield* set3(currentRef, null);
    }), "clearActive");
    const processNext = fn2("ScriptDialogs.processNext")(function* () {
      const pending2 = yield* take3(queue);
      if (yield* isDone2(pending2.canceled)) return;
      yield* set2(activeRef, pending2);
      yield* set3(currentRef, snapshotRequest(pending2.request));
      yield* raceFirst2(
        _await(pending2.response).pipe(asVoid2),
        _await(pending2.canceled)
      ).pipe(ensuring2(clearActive(pending2)));
    });
    yield* gen2(function* () {
      while (true) {
        yield* processNext();
      }
    }).pipe(forkIn2(scope3));
    const enqueue = fn2("ScriptDialogs.enqueue")(function* (request) {
      return yield* acquireUseRelease2(
        gen2(function* () {
          const id = yield* updateAndGet(nextIdRef, (current) => current + 1);
          const pending2 = {
            canceled: yield* make5(),
            request: requestWithId(request, id),
            response: yield* make5()
          };
          yield* offer(queue, pending2);
          return pending2;
        }),
        (pending2) => _await(pending2.response),
        (pending2) => succeed5(pending2.canceled, void 0).pipe(asVoid2)
      );
    });
    const alert = /* @__PURE__ */ __name((source, message2) => enqueue({ ...source, kind: "alert", message: message2 }).pipe(asVoid2), "alert");
    const confirm = /* @__PURE__ */ __name((source, message2) => gen2(function* () {
      const response = yield* enqueue({ ...source, kind: "confirm", message: message2 });
      if (response.kind !== "confirm") {
        return yield* die3("Script dialog response kind did not match.");
      }
      return response.confirmed;
    }), "confirm");
    const prompt = /* @__PURE__ */ __name((source, message2, placeholder = "") => gen2(function* () {
      const response = yield* enqueue({
        ...source,
        placeholder,
        kind: "prompt",
        message: message2
      });
      if (response.kind !== "prompt") {
        return yield* die3("Script dialog response kind did not match.");
      }
      return response.value;
    }), "prompt");
    const respond = /* @__PURE__ */ __name((response) => gen2(function* () {
      const active = yield* get3(activeRef);
      if (active === null || !responseMatchesRequest(active.request, response)) {
        return false;
      }
      return yield* succeed5(active.response, response);
    }), "respond");
    const onCurrent = /* @__PURE__ */ __name((listener) => changes(currentRef).pipe(
      runForEach2(
        (request) => sync2(() => listener(snapshotCurrent(request)))
      ),
      forkIn2(scope3),
      map5((fiber2) => () => {
        runFork3(interrupt4(fiber2));
      })
    ), "onCurrent");
    return ScriptDialogs.of({
      beep: /* @__PURE__ */ __name((times) => sync2(() => playBeep(times)), "beep"),
      alert,
      confirm,
      getCurrent: /* @__PURE__ */ __name(() => get4(currentRef).pipe(map5(snapshotCurrent)), "getCurrent"),
      onCurrent,
      prompt,
      respond
    });
  });
  var layer6 = effect(ScriptDialogs, makeScriptDialogs());

  // packages/runtime/src/game/scripting/ScriptRunnerErrors.ts
  var ScriptLoadError = class extends TaggedError3()(
    "ScriptLoadError",
    {
      detail: String4,
      cause: optionalKey2(Defect())
    }
  ) {
    static {
      __name(this, "ScriptLoadError");
    }
    get message() {
      return this.detail;
    }
  };
  var ScriptExecutionError = class extends TaggedError3()(
    "ScriptExecutionError",
    {
      detail: String4,
      cause: optionalKey2(Defect())
    }
  ) {
    static {
      __name(this, "ScriptExecutionError");
    }
    get message() {
      return this.detail;
    }
  };
  var ScriptNotReadyError = class extends TaggedError3()(
    "ScriptNotReadyError",
    {
      detail: String4,
      missing: ArraySchema(String4)
    }
  ) {
    static {
      __name(this, "ScriptNotReadyError");
    }
    get message() {
      return this.detail;
    }
  };
  var ScriptStopSignal = class extends TaggedError3()(
    "ScriptStopSignal",
    {
      reason: optionalKey2(String4)
    }
  ) {
    static {
      __name(this, "ScriptStopSignal");
    }
    get message() {
      return this.reason ?? "Script stopped.";
    }
  };
  var scriptExitRequests = /* @__PURE__ */ new WeakMap();
  var makeScriptExitSignal = /* @__PURE__ */ __name((options) => {
    const signal = new ScriptStopSignal({ reason: "Requested by the script" });
    const request = {
      closeClient: options?.closeClient === true,
      logout: options?.logout === true
    };
    scriptExitRequests.set(signal, request);
    return signal;
  }, "makeScriptExitSignal");
  var getScriptExitRequest = /* @__PURE__ */ __name((signal) => scriptExitRequests.get(signal), "getScriptExitRequest");

  // packages/runtime/src/host/scriptOptions.ts
  var decodePatch = decodeUnknownEffect2(AccountScriptSettingsPatchSchema);
  var invalidOptions = /* @__PURE__ */ __name((cause) => new ScriptExecutionError({
    cause,
    detail: "script.options.update requires valid runtime options."
  }), "invalidOptions");
  var makeScriptOptionsApi = /* @__PURE__ */ __name((host4) => ({
    get: /* @__PURE__ */ __name(() => host4.read({ kind: "scriptOptions" }), "get"),
    reset: /* @__PURE__ */ __name(() => host4.command({ kind: "resetScriptOptions" }).pipe(
      flatMap3(
        (result2) => isSome2(result2) ? void_3 : die3(new Error("Could not reset script options."))
      )
    ), "reset"),
    update: /* @__PURE__ */ __name((patch) => decodePatch(patch).pipe(
      mapError2(invalidOptions),
      flatMap3((patch2) => host4.command({ kind: "updateScriptOptions", patch: patch2 })),
      flatMap3(
        (result2) => isSome2(result2) ? void_3 : fail5(invalidOptions("Host rejected script options."))
      )
    ), "update")
  }), "makeScriptOptionsApi");

  // packages/runtime/src/game/scripting/scriptAsyncScope.ts
  var runCleanup = /* @__PURE__ */ __name((cleanup) => suspend2(() => {
    const result2 = cleanup();
    return isEffect2(result2) ? result2.pipe(asVoid2) : void_3;
  }).pipe(
    catchCause2(
      (cause) => logWarning("script cleanup failed", cause).pipe(
        annotateLogs({ component: "scripting" })
      )
    )
  ), "runCleanup");
  var makeScriptAsyncScope = /* @__PURE__ */ __name(() => {
    const controller = new AbortController();
    const cleanups = /* @__PURE__ */ new Set();
    const cancel = sync2(() => {
      if (!controller.signal.aborted) {
        controller.abort();
      }
    });
    const close2 = gen2(function* () {
      yield* cancel;
      const pending2 = Array.from(cleanups).reverse();
      cleanups.clear();
      yield* forEach2(pending2, runCleanup, { discard: true });
    });
    return {
      addCleanup: /* @__PURE__ */ __name((cleanup) => suspend2(() => {
        if (controller.signal.aborted) {
          return runCleanup(cleanup);
        }
        cleanups.add(cleanup);
        return void_3;
      }), "addCleanup"),
      cancel,
      close: close2,
      controller,
      signal: controller.signal
    };
  }, "makeScriptAsyncScope");

  // packages/runtime/src/game/scripting/scriptEntry.ts
  var isGeneratorFunction = /* @__PURE__ */ __name((value) => typeof value === "function" && value.constructor?.name === "GeneratorFunction", "isGeneratorFunction");
  var readScriptEntry = fn2("ScriptEntry.read")(function* (namespace, name, suppliedValues) {
    const exports = typeof namespace === "object" && namespace !== null ? namespace : {};
    const main = Reflect.get(exports, "default");
    if (!isGeneratorFunction(main)) {
      return yield* new ScriptLoadError({
        detail: `${name} must export a generator function as its default export.`
      });
    }
    const inputs = Reflect.get(exports, "inputs");
    if (inputs === void 0) return { main, inputValues: {} };
    const definition = yield* decodeUnknownEffect2(
      ScriptInputsDefinitionSchema
    )(inputs).pipe(
      mapError2(
        (cause) => new ScriptLoadError({
          detail: `${name} has an invalid inputs export.`,
          cause
        })
      )
    );
    const validation = validateScriptInputValues(definition, suppliedValues);
    if (validation.status === "missing-required") {
      return yield* new ScriptLoadError({
        detail: `${name} is missing required inputs: ${validation.fieldKeys.join(", ")}.`
      });
    }
    return { main, inputValues: validation.values };
  });

  // packages/runtime/src/game/scripting/scriptSourceAttribution.ts
  var SCRIPT_SOURCE_PATTERN = /lucent-script:\/\/(?:loose|package)\/([^\s:?#]+)/g;
  var SCRIPT_FRAME_PATTERN = /lucent-script:\/\/(?:loose|package)\/[^\s:?#]+:\d+:\d+/;
  var decodeSegment = /* @__PURE__ */ __name((value) => {
    try {
      return decodeURIComponent(value);
    } catch {
      return value;
    }
  }, "decodeSegment");
  var displayScriptSourceText = /* @__PURE__ */ __name((text2) => text2.replace(
    SCRIPT_SOURCE_PATTERN,
    (_url, identity2) => identity2.split("/").filter(Boolean).map(decodeSegment).join("/")
  ), "displayScriptSourceText");
  var errorChain = /* @__PURE__ */ __name((error) => {
    const chain = [];
    const seen = /* @__PURE__ */ new Set();
    let current = error;
    while (current !== void 0 && !seen.has(current)) {
      seen.add(current);
      chain.push(current);
      const cause = Reflect.get(current, "cause");
      current = cause instanceof Error ? cause : void 0;
    }
    return chain;
  }, "errorChain");
  var attributedScriptErrorMessage = /* @__PURE__ */ __name((error) => {
    const message2 = displayScriptSourceText(error.message);
    if (SCRIPT_FRAME_PATTERN.test(error.message)) return message2;
    for (const entry of errorChain(error)) {
      const frame = entry.stack?.match(SCRIPT_FRAME_PATTERN);
      if (frame) return `${displayScriptSourceText(frame[0])}: ${message2}`;
    }
    return message2;
  }, "attributedScriptErrorMessage");
  var attributedScriptErrorDetails = /* @__PURE__ */ __name((error) => {
    const details = errorChain(error).map(
      (entry) => displayScriptSourceText(
        entry.stack?.trim() === "" ? entry.message : entry.stack ?? entry.message
      )
    ).filter((entry) => entry.trim() !== "");
    return details.length === 0 ? void 0 : details.join("\nCaused by:\n");
  }, "attributedScriptErrorDetails");

  // packages/runtime/src/game/scripting/ScriptRuntime.ts
  var snapshotRoomPolicy = /* @__PURE__ */ __name((policy) => ({
    ...policy
  }), "snapshotRoomPolicy");
  var snapshotScriptInputValue = /* @__PURE__ */ __name((value) => Array.isArray(value) ? [...value] : value, "snapshotScriptInputValue");
  var snapshotScriptInputValues = /* @__PURE__ */ __name((values) => {
    const snapshot = {};
    for (const [key, value] of Object.entries(values)) {
      snapshot[key] = snapshotScriptInputValue(value);
    }
    return snapshot;
  }, "snapshotScriptInputValues");
  var makeScriptRuntimeApi = /* @__PURE__ */ __name((options) => {
    const source = { ...options.source };
    const script = {
      alert: /* @__PURE__ */ __name((message2) => options.dialogs.alert(source, message2), "alert"),
      beep: /* @__PURE__ */ __name((times = 1) => Number.isFinite(times) ? options.dialogs.beep(Math.max(1, Math.floor(times))) : fail5(
        new ScriptExecutionError({
          detail: "script.beep requires a finite number of repetitions."
        })
      ), "beep"),
      confirm: /* @__PURE__ */ __name((message2) => options.dialogs.confirm(source, message2), "confirm"),
      exit: /* @__PURE__ */ __name((exitOptions) => fail5(makeScriptExitSignal(exitOptions)), "exit"),
      inputs: Object.freeze({
        get: /* @__PURE__ */ __name((key) => sync2(() => {
          const value = options.getInputValues()[key];
          return value === void 0 ? void 0 : snapshotScriptInputValue(value);
        }), "get"),
        getAll: /* @__PURE__ */ __name(() => sync2(() => snapshotScriptInputValues(options.getInputValues())), "getAll")
      }),
      log: /* @__PURE__ */ __name((...values) => sync2(() => options.log(...values)), "log"),
      options: Object.freeze(options.scriptOptions),
      prompt: /* @__PURE__ */ __name((message2, placeholder) => options.dialogs.prompt(source, message2, placeholder), "prompt"),
      signal: options.scope.signal,
      sleep: /* @__PURE__ */ __name((duration) => try_2({
        try: /* @__PURE__ */ __name(() => toMillis(duration), "try"),
        catch: /* @__PURE__ */ __name((cause) => new ScriptExecutionError({
          cause,
          detail: "Invalid duration."
        }), "catch")
      }).pipe(
        flatMap3(
          (milliseconds) => Number.isFinite(milliseconds) && milliseconds >= 0 ? sleep2(duration) : fail5(
            new ScriptExecutionError({
              detail: "Duration must be non-negative and finite."
            })
          )
        )
      ), "sleep"),
      stop: /* @__PURE__ */ __name((reason) => fail5(new ScriptStopSignal(reason === void 0 ? {} : { reason })), "stop")
    };
    return Object.freeze(script);
  }, "makeScriptRuntimeApi");

  // packages/runtime/src/game/army/ArmyErrors.ts
  var ArmyError = class extends Error {
    static {
      __name(this, "ArmyError");
    }
    _tag = "ArmyError";
    constructor(message2, cause) {
      super(message2);
      this.name = "ArmyError";
      if (cause !== void 0) {
        Object.defineProperty(this, "cause", {
          configurable: true,
          enumerable: false,
          value: cause,
          writable: true
        });
      }
    }
  };
  var ArmyLoopTauntError = class extends Error {
    static {
      __name(this, "ArmyLoopTauntError");
    }
    _tag = "ArmyLoopTauntError";
    constructor(message2, cause) {
      super(message2);
      this.name = "ArmyLoopTauntError";
      if (cause !== void 0) {
        Object.defineProperty(this, "cause", {
          configurable: true,
          enumerable: false,
          value: cause,
          writable: true
        });
      }
    }
  };

  // packages/runtime/src/game/scripting/api/Callbacks.ts
  var isGenerator = /* @__PURE__ */ __name((value) => typeof value === "object" && value !== null && typeof value.next === "function" && typeof value.throw === "function", "isGenerator");
  var normalizeScriptCallbackResult = /* @__PURE__ */ __name((result2) => {
    if (isEffect2(result2)) {
      return result2;
    }
    if (isGenerator(result2)) {
      return gen2(function* () {
        const iterator = result2;
        return yield* iterator;
      });
    }
    return fail5(
      new ScriptExecutionError({
        detail: "Script callbacks must return an Effect or generator; plain values and Promises are not supported."
      })
    );
  }, "normalizeScriptCallbackResult");
  var normalizeScriptCallback = /* @__PURE__ */ __name((callback3, value) => try_2({
    try: /* @__PURE__ */ __name(() => callback3(value), "try"),
    catch: /* @__PURE__ */ __name((cause) => new ScriptExecutionError({
      detail: "Script callback threw before returning an Effect.",
      cause
    }), "catch")
  }).pipe(flatMap3(normalizeScriptCallbackResult), asVoid2), "normalizeScriptCallback");
  var notifyScriptCallbackFailure = /* @__PURE__ */ __name((callback3, failCause7) => (value) => normalizeScriptCallback(callback3, value).pipe(
    catchCause2(
      (cause) => failCause7(cause).pipe(andThen2(failCause4(cause)))
    )
  ), "notifyScriptCallbackFailure");

  // packages/runtime/src/game/scripting/api/Army.ts
  var normalizeSkipWhen = /* @__PURE__ */ __name((skipWhen) => (context3) => try_2({
    try: /* @__PURE__ */ __name(() => skipWhen(context3), "try"),
    catch: /* @__PURE__ */ __name((cause) => new ScriptExecutionError({
      cause,
      detail: "Loop Taunt skipWhen callback threw."
    }), "catch")
  }).pipe(
    flatMap3(
      (result2) => typeof result2 === "boolean" ? succeed7(result2) : normalizeScriptCallbackResult(result2)
    ),
    flatMap3(
      (result2) => typeof result2 === "boolean" ? succeed7(result2) : fail5(
        new ScriptExecutionError({
          detail: "Loop Taunt skipWhen must return a boolean, an Effect<boolean>, or a generator that returns a boolean."
        })
      )
    )
  ), "normalizeSkipWhen");
  var makeScriptArmyApi = /* @__PURE__ */ __name((army, scope3, failCause7) => {
    return Object.freeze({
      equipSet: army.equipSet,
      getConfigString: army.getConfigString,
      getConfigValue: army.getConfigValue,
      getPlayerNumber: army.getPlayerNumber,
      getSession: army.getSession,
      isLeader: army.isLeader,
      isMember: army.isMember,
      isStarted: army.isStarted,
      joinMap: army.joinMap,
      kill: army.kill,
      killForItem: army.killForItem,
      killForTempItem: army.killForTempItem,
      leave: army.leave,
      loopTaunt: /* @__PURE__ */ __name((plan) => try_2({
        try: /* @__PURE__ */ __name(() => plan.map((priorityGroup) => ({
          assignments: priorityGroup.assignments.map(
            (assignment) => {
              if (assignment.skipWhen === void 0) {
                return {
                  players: assignment.players,
                  strategy: assignment.strategy,
                  target: assignment.target
                };
              }
              if (typeof assignment.skipWhen !== "function") {
                throw new ArmyLoopTauntError(
                  "Loop Taunt skipWhen must be a function"
                );
              }
              return {
                ...assignment,
                skipWhen: normalizeSkipWhen(assignment.skipWhen)
              };
            }
          )
        })), "try"),
        catch: /* @__PURE__ */ __name((cause) => cause instanceof ArmyLoopTauntError ? cause : new ArmyLoopTauntError(
          "Failed to normalize Loop Taunt plan",
          cause
        ), "catch")
      }).pipe(
        flatMap3((normalized) => army.loopTaunt(normalized, failCause7)),
        // addCleanup runs immediately after cancellation, closing the race
        // between handle acquisition and an already-closed script scope.
        tap2((handle) => scope3.addCleanup(handle.stop))
      ), "loopTaunt"),
      runStep: army.runStep,
      start: army.start,
      sync: army.sync,
      waitForAllInMap: army.waitForAllInMap
    });
  }, "makeScriptArmyApi");

  // packages/runtime/src/game/scripting/api/Combat.ts
  var makeScriptCombatApi = /* @__PURE__ */ __name((combat) => {
    const killForItem = /* @__PURE__ */ __name((target2, goal, options) => combat.killForItem(target2, goal.item, goal.quantity, options), "killForItem");
    const killForTempItem = /* @__PURE__ */ __name((target2, goal, options) => combat.killForTempItem(target2, goal.item, goal.quantity, options), "killForTempItem");
    const target = Object.freeze({
      auras: Object.freeze({
        get: combat.target.auras.get,
        getAll: combat.target.auras.getAll,
        has: combat.target.auras.has
      }),
      get: combat.target.get
    });
    return Object.freeze({
      attack: combat.attack,
      cancelAutoAttack: combat.cancelAutoAttack,
      cancelTarget: combat.cancelTarget,
      canUseSkill: combat.canUseSkill,
      exit: combat.exit,
      getSkillCooldownRemainingMs: combat.getSkillCooldownRemainingMs,
      hunt: combat.hunt,
      kill: combat.kill,
      killForItem,
      killForTempItem,
      target,
      useSkill: combat.useSkill
    });
  }, "makeScriptCombatApi");

  // packages/runtime/src/game/scripting/api/Events.ts
  var makeScriptEventsApi = /* @__PURE__ */ __name((events, scope3, failCause7) => {
    const on = /* @__PURE__ */ __name(((selector, handler) => {
      return events.onProjection(selector, notifyScriptCallbackFailure(handler, failCause7)).pipe(tap2((dispose) => scope3.addCleanup(dispose)));
    }), "on");
    const once = events.onceProjection;
    return Object.freeze({
      on,
      once
    });
  }, "makeScriptEventsApi");

  // packages/runtime/src/game/scripting/api/Environment.ts
  var ScriptEnvironmentError = class extends TaggedError3()(
    "EnvironmentError",
    {
      cause: Defect(),
      operation: String4
    }
  ) {
    static {
      __name(this, "ScriptEnvironmentError");
    }
    get message() {
      return `Environment ${this.operation} failed.`;
    }
  };
  var toEnvironmentSnapshot = /* @__PURE__ */ __name((state) => Object.freeze({
    automation: Object.freeze({ ...state.automation }),
    boosts: Object.freeze([...state.boosts]),
    dropPolicy: Object.freeze(
      environmentItemRulesToDropPolicy(state.itemRules)
    ),
    itemNames: Object.freeze([...state.itemNames]),
    itemNotificationNames: Object.freeze([...state.itemNotificationNames]),
    questAutoRegister: Object.freeze({ ...state.questAutoRegister }),
    questIds: Object.freeze([...state.questIds]),
    questRewards: Object.freeze({ ...state.questRewards })
  }), "toEnvironmentSnapshot");
  var exposeMutation = /* @__PURE__ */ __name((operation, effect2) => effect2.pipe(
    map5(toEnvironmentSnapshot),
    mapError2(
      (cause) => new ScriptEnvironmentError({ cause, operation })
    )
  ), "exposeMutation");
  var makeScriptEnvironmentApi = /* @__PURE__ */ __name((environment) => {
    const automationSetters = {
      boosts: environment.setBoostAutomationEnabled,
      drops: environment.setDropAutomationEnabled,
      quests: environment.setQuestAutomationEnabled
    };
    const api2 = {
      getState: /* @__PURE__ */ __name(() => environment.getState().pipe(map5(toEnvironmentSnapshot)), "getState"),
      clearRegistrations: /* @__PURE__ */ __name(() => exposeMutation("clearRegistrations", environment.clear()), "clearRegistrations"),
      setAutomationEnabled: /* @__PURE__ */ __name((capability, enabled) => exposeMutation(
        "setAutomationEnabled",
        automationSetters[capability](enabled)
      ), "setAutomationEnabled"),
      addQuest: /* @__PURE__ */ __name((questId, rewardItemId) => exposeMutation("addQuest", environment.addQuest(questId, rewardItemId)), "addQuest"),
      removeQuest: /* @__PURE__ */ __name((questId) => exposeMutation("removeQuest", environment.removeQuest(questId)), "removeQuest"),
      setQuestReward: /* @__PURE__ */ __name((questId, rewardItemId) => exposeMutation(
        "setQuestReward",
        environment.setQuestReward(questId, rewardItemId)
      ), "setQuestReward"),
      clearQuestReward: /* @__PURE__ */ __name((questId) => exposeMutation("clearQuestReward", environment.clearQuestReward(questId)), "clearQuestReward"),
      clearQuests: /* @__PURE__ */ __name(() => exposeMutation("clearQuests", environment.clearQuests()), "clearQuests"),
      updateQuestAutoRegister: /* @__PURE__ */ __name((patch) => exposeMutation(
        "updateQuestAutoRegister",
        environment.setQuestAutoRegister(patch)
      ), "updateQuestAutoRegister"),
      addItem: /* @__PURE__ */ __name((name) => exposeMutation("addItem", environment.addItem(name)), "addItem"),
      removeItem: /* @__PURE__ */ __name((name) => exposeMutation("removeItem", environment.removeItem(name)), "removeItem"),
      clearItems: /* @__PURE__ */ __name(() => exposeMutation("clearItems", environment.clearItems()), "clearItems"),
      setItemNotification: /* @__PURE__ */ __name((name, enabled) => exposeMutation(
        "setItemNotification",
        environment.setItemNotification(name, enabled)
      ), "setItemNotification"),
      updateDropPolicy: /* @__PURE__ */ __name((patch) => exposeMutation("updateDropPolicy", environment.setDropPolicy(patch)), "updateDropPolicy"),
      addBoost: /* @__PURE__ */ __name((name) => exposeMutation("addBoost", environment.addBoost(name)), "addBoost"),
      removeBoost: /* @__PURE__ */ __name((name) => exposeMutation("removeBoost", environment.removeBoost(name)), "removeBoost"),
      clearBoosts: /* @__PURE__ */ __name(() => exposeMutation("clearBoosts", environment.clearBoosts()), "clearBoosts")
    };
    return Object.freeze(api2);
  }, "makeScriptEnvironmentApi");

  // packages/runtime/src/game/scripting/api/Packet.ts
  var makeScriptPacketApi = /* @__PURE__ */ __name((packet, scope3, failCause7) => {
    const on = /* @__PURE__ */ __name(((selector, handler) => packet.on(selector, notifyScriptCallbackFailure(handler, failCause7)).pipe(
      tap2((dispose) => scope3.addCleanup(dispose))
    )), "on");
    return Object.freeze({
      on,
      once: packet.once,
      sendToClient: packet.sendToClient,
      sendToServer: packet.sendToServer
    });
  }, "makeScriptPacketApi");

  // packages/runtime/src/game/scripting/api/Players.ts
  var makeScriptPlayersApi = /* @__PURE__ */ __name((players) => Object.freeze({
    get: /* @__PURE__ */ __name((query) => players.get(toPlayerSelector(query).username), "get"),
    getAll: players.getAll
  }), "makeScriptPlayersApi");

  // packages/runtime/src/game/scripting/api/Player.ts
  var makeScriptPlayerJoinMap = /* @__PURE__ */ __name((joinMap, script) => fn2("ScriptPlayer.joinMap")(function* (map8, options) {
    const policy = yield* script.policy;
    if (policy.kind === "random-private" && (yield* script.map.isLoaded())) {
      const currentName = yield* script.map.getName();
      const currentRoom = yield* script.map.getRoomNumber();
      if (map8.trim().toLowerCase() === currentName.trim().toLowerCase() && roomPolicyAcceptsRoom(policy, currentRoom)) {
        return yield* joinMap(withRoomNumber(map8.trim(), currentRoom), options);
      }
    }
    return yield* joinMap(yield* applyRoomPolicy(map8, policy), options);
  }), "makeScriptPlayerJoinMap");
  var makeScriptPlayerApis = /* @__PURE__ */ __name((player, players, script) => {
    const auras = Object.freeze({
      get: player.auras.get,
      getAll: player.auras.getAll,
      has: player.auras.has
    });
    const factions = Object.freeze({
      get: player.factions.get,
      getAll: player.factions.getAll
    });
    const outfits = Object.freeze({
      equip: player.outfits.equip,
      get: player.outfits.get,
      getAll: player.outfits.getAll,
      wear: player.outfits.wear
    });
    const playerApi = Object.freeze({
      auras,
      factions,
      get: player.get,
      getCell: player.getCell,
      getClassName: player.getClassName,
      getClassRank: player.getClassRank,
      getGender: player.getGender,
      getGold: player.getGold,
      getHp: player.getHp,
      getLevel: player.getLevel,
      getMaxHp: player.getMaxHp,
      getMaxMp: player.getMaxMp,
      getMp: player.getMp,
      getPad: player.getPad,
      getPosition: player.getPosition,
      getState: player.getState,
      goToPlayer: player.goToPlayer,
      hasActiveBoost: player.hasActiveBoost,
      isAfk: player.isAfk,
      isAlive: player.isAlive,
      isMember: player.isMember,
      isReady: player.isReady,
      joinMap: makeScriptPlayerJoinMap(
        (map8, options) => player.joinMap(map8, options),
        script
      ),
      jumpToCell: player.jumpToCell,
      outfits,
      rest: player.rest,
      walkTo: player.walkTo
    });
    return {
      player: playerApi,
      players: makeScriptPlayersApi(players)
    };
  }, "makeScriptPlayerApis");

  // packages/runtime/src/game/scripting/api/PublicServices.ts
  var makeScriptPublicServices = /* @__PURE__ */ __name((services) => ({
    auth: Object.freeze({
      connectTo: services.auth.connectTo,
      getPassword: services.auth.getPassword,
      getServers: services.auth.getServers,
      getUsername: services.auth.getUsername,
      isLoggedIn: services.auth.isLoggedIn,
      isServerSelectReady: services.auth.isServerSelectReady,
      isTemporarilyKicked: services.auth.isTemporarilyKicked,
      login: services.auth.login,
      logout: /* @__PURE__ */ __name(() => services.auth.logout().pipe(asVoid2), "logout")
    }),
    bank: Object.freeze({
      contains: services.bank.contains,
      deposit: services.bank.deposit,
      depositBatch: services.bank.depositBatch,
      get: services.bank.get,
      getAll: services.bank.getAll,
      getAvailableSlots: services.bank.getAvailableSlots,
      getSlots: services.bank.getSlots,
      getUsedSlots: services.bank.getUsedSlots,
      isOpen: services.bank.isOpen,
      load: services.bank.load,
      open: services.bank.open,
      swap: services.bank.swap,
      withdraw: services.bank.withdraw,
      withdrawBatch: services.bank.withdrawBatch
    }),
    drops: Object.freeze({
      accept: services.drops.accept,
      contains: services.drops.contains,
      get: services.drops.get,
      getAll: services.drops.getAll,
      reject: services.drops.reject
    }),
    house: Object.freeze({
      contains: services.house.contains,
      get: services.house.get,
      getAll: services.house.getAll,
      getAvailableSlots: services.house.getAvailableSlots,
      getSlots: services.house.getSlots,
      getUsedSlots: services.house.getUsedSlots
    }),
    inventory: Object.freeze({
      contains: services.inventory.contains,
      equip: services.inventory.equip,
      equipByEnhancement: services.inventory.equipByEnhancement,
      get: services.inventory.get,
      getAll: services.inventory.getAll,
      getAvailableSlots: services.inventory.getAvailableSlots,
      getSlots: services.inventory.getSlots,
      getUsedSlots: services.inventory.getUsedSlots,
      unequipConsumable: services.inventory.unequipConsumable,
      use: services.inventory.use,
      wear: services.inventory.wear
    }),
    map: Object.freeze({
      getCellPads: services.map.getCellPads,
      getCells: services.map.getCells,
      getId: services.map.getId,
      getMapItem: services.map.getMapItem,
      getName: services.map.getName,
      getRoomNumber: services.map.getRoomNumber,
      isLoaded: services.map.isLoaded,
      loadSwf: services.map.loadSwf,
      reload: services.map.reload,
      setSpawnPoint: services.map.setSpawnPoint
    }),
    monsters: Object.freeze({
      get: services.monsters.get,
      getAll: services.monsters.getAll,
      getAvailable: services.monsters.getAvailable,
      isAvailable: services.monsters.isAvailable
    }),
    quests: Object.freeze({
      abandon: services.quests.abandon,
      accept: services.quests.accept,
      acceptBatch: services.quests.acceptBatch,
      canComplete: services.quests.canComplete,
      complete: services.quests.complete,
      get: services.quests.get,
      getAccepted: services.quests.getAccepted,
      getAll: services.quests.getAll,
      getMaxTurnIns: services.quests.getMaxTurnIns,
      isAvailable: services.quests.isAvailable,
      isInProgress: services.quests.isInProgress,
      load: services.quests.load,
      loadBatch: services.quests.loadBatch
    }),
    tempInventory: Object.freeze({
      contains: services.tempInventory.contains,
      get: services.tempInventory.get,
      getAll: services.tempInventory.getAll
    }),
    wait: Object.freeze({
      forGameAction: services.wait.forGameAction,
      isGameActionAvailable: services.wait.isGameActionAvailable,
      until: services.wait.until,
      untilSome: services.wait.untilSome
    })
  }), "makeScriptPublicServices");

  // packages/runtime/src/game/scripting/api/Settings.ts
  var makeScriptSettingsApi = /* @__PURE__ */ __name((settings) => Object.freeze({
    update: /* @__PURE__ */ __name((patch) => settings.apply({
      ...patch.animations === void 0 ? {} : { animationsEnabled: patch.animations },
      ...patch.antiCounter === void 0 ? {} : { antiCounterEnabled: patch.antiCounter },
      ...patch.collisions === void 0 ? {} : { collisionsEnabled: patch.collisions },
      ...patch.customGuild === void 0 ? {} : { customGuild: patch.customGuild },
      ...patch.customName === void 0 ? {} : { customName: patch.customName },
      ...patch.deathAds === void 0 ? {} : { deathAdsVisible: patch.deathAds },
      ...patch.enemyMagnet === void 0 ? {} : { enemyMagnetEnabled: patch.enemyMagnet },
      ...patch.frameRate === void 0 ? {} : { frameRate: patch.frameRate },
      ...patch.infiniteRange === void 0 ? {} : { infiniteRangeEnabled: patch.infiniteRange },
      ...patch.hidePlayers === void 0 ? {} : { otherPlayersVisible: !patch.hidePlayers },
      ...patch.provokeCell === void 0 ? {} : { provokeCellEnabled: patch.provokeCell },
      ...patch.renderingMode === void 0 ? {} : { renderingMode: patch.renderingMode },
      ...patch.skipCutscenes === void 0 ? {} : { skipCutscenesEnabled: patch.skipCutscenes },
      ...patch.walkSpeed === void 0 ? {} : { walkSpeed: patch.walkSpeed }
    }), "update"),
    get: fn2("ScriptSettings.get")(function* () {
      const state = yield* settings.get();
      return Object.freeze({
        animations: state.animationsEnabled,
        antiCounter: state.antiCounterEnabled,
        collisions: state.collisionsEnabled,
        customGuild: state.customGuildConfigured ? state.customGuild : null,
        customName: state.customNameConfigured ? state.customName : null,
        deathAds: state.deathAdsVisible,
        enemyMagnet: state.enemyMagnetEnabled,
        frameRate: state.frameRate,
        infiniteRange: state.infiniteRangeEnabled,
        hidePlayers: !state.otherPlayersVisible,
        provokeCell: state.provokeCellEnabled,
        renderingMode: state.renderingMode,
        skipCutscenes: state.skipCutscenesEnabled,
        walkSpeed: state.walkSpeed
      });
    })
  }), "makeScriptSettingsApi");

  // packages/runtime/src/game/scripting/api/Shop.ts
  var enhancementResponseContains = /* @__PURE__ */ __name((packet, itemId) => {
    if (packet.direction === "client" || typeof packet.data !== "object") {
      return false;
    }
    const itemIds = packet.data.ItemIDs;
    return Array.isArray(itemIds) && itemIds.some((value) => Number(value) === itemId);
  }, "enhancementResponseContains");
  var candidateOrder = /* @__PURE__ */ __name((member) => (left, right) => {
    const levelDifference = (right.enhancement?.level ?? 0) - (left.enhancement?.level ?? 0);
    if (levelDifference !== 0) return levelDifference;
    if (member && left.memberOnly !== right.memberOnly) {
      return Number(right.memberOnly === member) - Number(left.memberOnly === member);
    }
    return (left.shopItemId ?? left.itemId) - (right.shopItemId ?? right.itemId);
  }, "candidateOrder");
  var selectorForCandidate = /* @__PURE__ */ __name((candidate) => candidate.shopItemId === void 0 ? { itemId: candidate.itemId } : { shopItemId: candidate.shopItemId }, "selectorForCandidate");
  var ScriptItemQuerySchema = Union2([
    PositiveInt,
    TrimmedNonEmptyString,
    Struct({
      itemId: PositiveInt,
      name: optionalKey2(Never2)
    }),
    Struct({
      itemId: optionalKey2(Never2),
      name: TrimmedNonEmptyString
    })
  ]);
  var ScriptEnhanceItemOptionsSchema = EnhancementSelectorSchema;
  var ScriptEnhanceItemRequestSchema = Struct({
    item: ScriptItemQuerySchema,
    options: ScriptEnhanceItemOptionsSchema
  });
  var decodeScriptEnhanceItemRequest = decodeUnknownOption2(
    ScriptEnhanceItemRequestSchema
  );
  var makeScriptShopApi = /* @__PURE__ */ __name((services, bridge) => {
    const enhanceItem = fn2("ScriptShop.enhanceItem")(function* (item, options) {
      const itemSelector = toItemSelector(item);
      const { enhancement, special } = options;
      const inventoryItem = yield* services.inventory.get(itemSelector);
      if (inventoryItem === null) {
        return false;
      }
      const playerLevel = yield* services.player.getLevel();
      const strategyResolution = resolveEnhancementStrategy(
        inventoryItem,
        enhancement,
        playerLevel,
        special
      );
      if (!strategyResolution.ok) {
        return false;
      }
      const strategy = strategyResolution.strategy;
      const matchesApplied = matchesAppliedEnhancement(inventoryItem, strategy);
      const appliedLevel = matchesApplied ? inventoryItem.enhancement?.level : void 0;
      if (appliedLevel !== void 0 && appliedLevel >= playerLevel) {
        return true;
      }
      if (strategy.map !== void 0) {
        const joined = yield* services.player.joinMap(strategy.map);
        if (!joined) return false;
      }
      const loaded = yield* services.shops.load(strategy.shopId);
      if (!loaded) return false;
      const member = yield* services.player.isMember();
      const shopItems = yield* services.shops.getAll();
      const candidates = shopItems.filter(
        (candidate) => (!candidate.memberOnly || member) && candidate.enhancement?.level !== void 0 && candidate.enhancement.level <= playerLevel && matchesEnhancementShopItem(candidate, strategy)
      ).toSorted(candidateOrder(member));
      let candidateSelector;
      for (const current of candidates) {
        const candidateLevel = current.enhancement?.level ?? 0;
        if (appliedLevel !== void 0 && appliedLevel >= candidateLevel) {
          return true;
        }
        const selector = selectorForCandidate(current);
        const canBuy = yield* services.shops.canBuy(selector);
        if (!canBuy) continue;
        candidateSelector = selector;
        break;
      }
      if (candidateSelector === void 0) {
        return false;
      }
      const actionAvailable = yield* services.wait.forGameAction("buyItem");
      if (!actionAvailable) return false;
      const response = yield* services.wait.forPacket(
        {
          command: "enhanceItemShop",
          direction: "extension",
          predicate: /* @__PURE__ */ __name((packet) => enhancementResponseContains(packet, inventoryItem.itemId), "predicate"),
          encoding: "json"
        },
        {
          timeout: "5 seconds",
          trigger: gen2(function* () {
            const bridgeResult = yield* bridge.invoke(
              "shops.enhance",
              [candidateSelector, inventoryItem.itemId],
              Boolean3
            );
            return getOrElse(bridgeResult, () => false);
          })
        }
      );
      if (response === null) {
        return false;
      }
      const updated = yield* services.inventory.get(inventoryItem.itemId);
      return updated !== null && matchesAppliedEnhancement(updated, strategy);
    });
    const shop = {
      buy: services.shops.buy,
      canBuy: services.shops.canBuy,
      close: services.shops.close,
      enhanceItem: /* @__PURE__ */ __name((item, options) => match(decodeScriptEnhanceItemRequest({ item, options }), {
        onNone: /* @__PURE__ */ __name(() => succeed7(false), "onNone"),
        onSome: /* @__PURE__ */ __name((request) => enhanceItem(request.item, request.options), "onSome")
      }), "enhanceItem"),
      get: services.shops.get,
      getAll: services.shops.getAll,
      getCurrent: services.shops.getCurrent,
      getMaxBuyQuantity: services.shops.getMaxBuyQuantity,
      isMergeShop: services.shops.isMergeShop,
      isOpen: services.shops.isOpen,
      open: services.shops.load,
      openArmorCustomize: services.shops.openArmorCustomize,
      openHairShop: services.shops.openHairShop,
      sell: services.shops.sell
    };
    return Object.freeze(shop);
  }, "makeScriptShopApi");

  // packages/runtime/src/game/scripting/ScriptEffectStd.ts
  var scriptDurationModule = {
    days,
    hours,
    millis,
    minutes,
    seconds,
    toMillis
  };
  Object.freeze(scriptDurationModule);
  function scriptEffectAll(effects) {
    return all2(effects);
  }
  __name(scriptEffectAll, "scriptEffectAll");
  var scriptEffectModule = {
    all: scriptEffectAll,
    as: as2,
    asVoid: asVoid2,
    catch: catch_2,
    fail: fail5,
    flatMap: flatMap3,
    forEach: /* @__PURE__ */ __name((values, transform3) => forEach2(values, transform3), "forEach"),
    gen: gen2,
    map: map5,
    mapError: mapError2,
    raceFirst: raceFirst2,
    sleep: sleep2,
    succeed: succeed7,
    sync: sync2,
    tap: tap2,
    timeoutOption: timeoutOption2,
    try: try_2,
    tryPromise: tryPromise2,
    void: void_3
  };
  Object.freeze(scriptEffectModule);
  var scriptOptionModule = {
    getOrElse,
    isNone: isNone2,
    isSome: isSome2,
    map,
    match,
    none: none2,
    some: some2
  };
  Object.freeze(scriptOptionModule);
  var scriptEffectStd = Object.freeze({
    Duration: scriptDurationModule,
    Effect: scriptEffectModule,
    Option: scriptOptionModule,
    pipe
  });

  // packages/runtime/src/game/scripting/schema/issues.ts
  var SchemaUsageError = class extends Error {
    static {
      __name(this, "SchemaUsageError");
    }
    name = "SchemaUsageError";
  };
  var assertSync = /* @__PURE__ */ __name((value) => {
    if (isPromiseLike(value)) {
      void Promise.resolve(value).catch(() => void 0);
      throw new SchemaUsageError(
        "Schema callbacks must return synchronously, without promises."
      );
    }
    return value;
  }, "assertSync");
  var formatPath2 = /* @__PURE__ */ __name((path) => {
    let result2 = "$";
    for (const part of path) {
      result2 += typeof part === "number" ? `[${part}]` : /^[A-Za-z_$][\w$]*$/.test(part) ? `.${part}` : `[${JSON.stringify(part)}]`;
    }
    return result2;
  }, "formatPath");
  var formatIssues = /* @__PURE__ */ __name((issues) => issues.map((issue2) => `${formatPath2(issue2.path)}: ${issue2.message}`).join("\n"), "formatIssues");
  var ValidationError = class extends Error {
    static {
      __name(this, "ValidationError");
    }
    name = "ValidationError";
    issues;
    constructor(issues) {
      super(formatIssues(issues));
      this.issues = issues;
    }
  };
  var diagnostics = /* @__PURE__ */ new WeakMap();
  var nativeFormatter = makeFormatterDefault();
  var issue = /* @__PURE__ */ __name((code, message2, path = []) => fromIssues([{ code, message: message2, path }]), "issue");
  var fromIssues = /* @__PURE__ */ __name((issues) => {
    const result2 = new InvalidValue({
      message: issues[0]?.message ?? "Invalid value."
    });
    diagnostics.set(result2, issues);
    return result2;
  }, "fromIssues");
  var atPath = /* @__PURE__ */ __name((path, cause) => new Pointer(path, cause), "atPath");
  var flatten6 = /* @__PURE__ */ __name((cause, options = {}, prefix = []) => {
    const output = [];
    const limit = options.maxIssues ?? 100;
    const rebase = /* @__PURE__ */ __name((entry, path) => ({
      ...entry,
      path: Object.freeze([...path, ...entry.path]),
      ...entry.branches === void 0 ? {} : {
        branches: Object.freeze(
          entry.branches.map(
            (branch) => Object.freeze(branch.map((child2) => rebase(child2, path)))
          )
        )
      }
    }), "rebase");
    const append3 = /* @__PURE__ */ __name((entry, path) => {
      if (output.length >= limit) return;
      const full = rebase(entry, path);
      const message2 = assertSync(options.message?.(full));
      if (message2 !== void 0 && typeof message2 !== "string") {
        throw new SchemaUsageError(
          "A message callback must return a string or undefined."
        );
      }
      output.push(
        Object.freeze(message2 === void 0 ? full : { ...full, message: message2 })
      );
    }, "append");
    const visit = /* @__PURE__ */ __name((entry, path) => {
      if (output.length >= limit) return;
      const own = diagnostics.get(entry);
      if (own !== void 0) {
        for (const diagnostic of own) append3(diagnostic, path);
        return;
      }
      switch (entry._tag) {
        case "Pointer":
          visit(entry.issue, [
            ...path,
            ...entry.path.map(
              (key) => typeof key === "symbol" ? String(key) : key
            )
          ]);
          return;
        case "Composite":
          for (const child2 of entry.issues) visit(child2, path);
          return;
        case "Filter":
        case "Encoding":
          visit(entry.issue, path);
          return;
        case "AnyOf":
          append3(
            {
              code: "invalid_union",
              path: [],
              message: "Expected one of the allowed values or shapes.",
              branches: entry.issues.map(
                (child2) => flatten6(child2, { maxIssues: limit })
              )
            },
            path
          );
          return;
        case "Forbidden":
          throw new SchemaUsageError(
            "Schema callbacks must complete synchronously."
          );
        default: {
          const code = entry._tag === "MissingKey" ? "missing" : entry._tag === "UnexpectedKey" ? "unrecognized_key" : entry._tag === "InvalidType" ? "invalid_type" : "invalid_value";
          append3({ code, path: [], message: nativeFormatter(entry) }, path);
        }
      }
    }, "visit");
    visit(cause, prefix);
    return Object.freeze(output);
  }, "flatten");

  // packages/runtime/src/game/scripting/schema/engine.ts
  var MISSING = /* @__PURE__ */ Symbol("lucent/schema/missing");
  var INVALID = /* @__PURE__ */ Symbol("lucent/schema/invalid");
  var stateKey = /* @__PURE__ */ Symbol("lucent/schema/parse-state");
  var definitions = /* @__PURE__ */ new WeakMap();
  var parsers = /* @__PURE__ */ new WeakMap();
  var register = /* @__PURE__ */ __name((instance, definition) => {
    definitions.set(instance, Object.freeze(definition));
  }, "register");
  var isSchema2 = /* @__PURE__ */ __name((value) => isObject(value) && definitions.has(value), "isSchema");
  var definitionOf = /* @__PURE__ */ __name((value) => {
    const definition = isObject(value) ? definitions.get(value) : void 0;
    if (definition === void 0)
      throw new SchemaUsageError("Expected a lucent/schema schema.");
    return definition;
  }, "definitionOf");
  var missingOf = /* @__PURE__ */ __name((definition) => typeof definition.missing === "function" ? definition.missing() : definition.missing, "missingOf");
  var stateOf = /* @__PURE__ */ __name((options) => {
    const state = options[stateKey];
    if (state === void 0)
      throw new SchemaUsageError("Use the schema's parsing methods.");
    return state;
  }, "stateOf");
  var makeOptions = /* @__PURE__ */ __name((options = {}) => {
    if (!isObject(options))
      throw new SchemaUsageError("Expected parsing options.");
    const errors = options.errors ?? "first";
    const maxIssues = options.maxIssues ?? 100;
    const maxDepth = options.maxDepth ?? 256;
    if (errors !== "first" && errors !== "all")
      throw new SchemaUsageError('errors must be "first" or "all".');
    positiveInteger(maxIssues, "maxIssues");
    positiveInteger(maxDepth, "maxDepth");
    if (options.message !== void 0 && !isFunction(options.message)) {
      throw new SchemaUsageError("message must be a function.");
    }
    return {
      errors,
      reportInput: false,
      [stateKey]: {
        path: [],
        ancestors: /* @__PURE__ */ new Set(),
        maxDepth,
        maxIssues,
        errors,
        depth: 0
      }
    };
  }, "makeOptions");
  var decode2 = /* @__PURE__ */ __name((backend, input, options) => {
    let parser = parsers.get(backend);
    if (parser === void 0) {
      parser = decodeUnknownResult(backend);
      parsers.set(backend, parser);
    }
    return parser(input, options);
  }, "decode");
  var declare2 = /* @__PURE__ */ __name((run2) => declareConstructor()([], () => (input, _ast, options) => {
    const result2 = run2(input, options);
    return isSuccess2(result2) ? succeed7(result2.success) : fail5(result2.failure);
  }), "declare");
  var field = /* @__PURE__ */ __name((definition, input, options) => {
    if (input === MISSING) {
      const missing2 = missingOf(definition);
      if (missing2 === "omit") return succeed2(MISSING);
      if (missing2 === "reject")
        return fail2(issue("missing", "This field is required."));
    }
    return decode2(definition.backend, input, options);
  }, "field");
  var child = /* @__PURE__ */ __name((definition, input, key, options) => {
    const state = stateOf(options);
    state.path.push(key);
    try {
      const result2 = field(definition, input, options);
      return isFailure2(result2) ? fail2(atPath([key], result2.failure)) : result2;
    } finally {
      state.path.pop();
    }
  }, "child");
  var container = /* @__PURE__ */ __name((input, options, run2) => {
    const state = stateOf(options);
    if (state.depth >= state.maxDepth)
      return fail2(
        issue("max_depth", "The value exceeds the maximum nesting depth.")
      );
    if (state.ancestors.has(input))
      return fail2(
        issue("cyclic_reference", "Cyclic values cannot be parsed.")
      );
    state.depth += 1;
    state.ancestors.add(input);
    try {
      return run2();
    } finally {
      state.depth -= 1;
      state.ancestors.delete(input);
    }
  }, "container");
  var canContinue = /* @__PURE__ */ __name((issues, options) => {
    const state = stateOf(options);
    return issues.length === 0 || state.errors === "all" && issues.length < state.maxIssues;
  }, "canContinue");
  var collect = /* @__PURE__ */ __name((issues, cause, options) => {
    const state = stateOf(options);
    issues.push(
      ...flatten6(cause, {
        maxIssues: Math.max(1, state.maxIssues - issues.length)
      })
    );
  }, "collect");
  var message = /* @__PURE__ */ __name((options, fallback) => {
    if (options !== void 0 && (!isObject(options) || options.message !== void 0 && typeof options.message !== "string")) {
      throw new SchemaUsageError("Expected a string validation message.");
    }
    return options?.message ?? fallback;
  }, "message");
  var size = /* @__PURE__ */ __name((value, name = "length") => {
    if (!Number.isSafeInteger(value) || value < 0)
      throw new SchemaUsageError(`${name} must be a nonnegative safe integer.`);
  }, "size");
  var positiveInteger = /* @__PURE__ */ __name((value, name) => {
    size(value, name);
    if (value === 0)
      throw new SchemaUsageError(`${name} must be greater than zero.`);
  }, "positiveInteger");
  var finite2 = /* @__PURE__ */ __name((value) => {
    if (!Number.isFinite(value))
      throw new SchemaUsageError("Expected a finite numeric bound.");
  }, "finite");
  var validatePath = /* @__PURE__ */ __name((path) => {
    if (!Array.isArray(path) || !path.every(
      (part) => typeof part === "string" || typeof part === "number" && Number.isSafeInteger(part) && part >= 0
    )) {
      throw new SchemaUsageError(
        "An issue path must contain strings or nonnegative integer indices."
      );
    }
    return Object.freeze([...path]);
  }, "validatePath");
  var baseDefinition = /* @__PURE__ */ __name((backend) => ({
    backend,
    missing: "reject",
    metadata: Object.freeze({})
  }), "baseDefinition");
  var writeProperty = /* @__PURE__ */ __name((target, key, value) => {
    Object.defineProperty(target, key, {
      configurable: true,
      enumerable: true,
      writable: true,
      value
    });
  }, "writeProperty");

  // packages/runtime/src/game/scripting/schema/Schema.ts
  var SchemaImpl = class {
    static {
      __name(this, "SchemaImpl");
    }
    constructor(definition) {
      register(this, definition);
    }
    get description() {
      return definitionOf(this).description;
    }
    get metadata() {
      return definitionOf(this).metadata;
    }
    parse(input, options) {
      const result2 = this.safeParse(input, options);
      if (!result2.success) throw new ValidationError(result2.issues);
      return result2.data;
    }
    safeParse(input, options) {
      const engineOptions = makeOptions(options);
      const result2 = decode2(definitionOf(this).backend, input, engineOptions);
      return isSuccess2(result2) ? { success: true, data: result2.success } : {
        success: false,
        issues: flatten6(result2.failure, {
          ...options,
          maxIssues: engineOptions.errors === "first" ? 1 : stateOf(engineOptions).maxIssues
        })
      };
    }
    is(input) {
      return isSuccess2(
        decode2(definitionOf(this).backend, input, makeOptions())
      );
    }
    optional() {
      const previous = definitionOf(this);
      return create({
        ...previous,
        missing: "omit",
        backend: declare2(
          (value, options) => value === void 0 ? succeed2(void 0) : decode2(previous.backend, value, options)
        )
      });
    }
    optionalKey() {
      return create({ ...definitionOf(this), missing: "omit" });
    }
    nullable() {
      const previous = definitionOf(this);
      return create({
        ...previous,
        literals: previous.literals === void 0 ? void 0 : Object.freeze([.../* @__PURE__ */ new Set([...previous.literals, null])]),
        backend: declare2(
          (value, options) => value === null ? succeed2(null) : decode2(previous.backend, value, options)
        )
      });
    }
    nullish() {
      return this.nullable().optional();
    }
    required() {
      return this.rejectValue(void 0, "A value is required.", "reject");
    }
    nonnullable() {
      return this.rejectValue(
        null,
        "Null is not allowed.",
        definitionOf(this).missing
      );
    }
    rejectValue(rejected2, text2, missing2) {
      const previous = definitionOf(this);
      return create({
        ...previous,
        missing: missing2,
        literals: previous.literals?.filter((value) => value !== rejected2),
        backend: declare2((value, options) => {
          if (value === rejected2)
            return fail2(issue("invalid_value", text2));
          const result2 = decode2(previous.backend, value, options);
          return isSuccess2(result2) && result2.success === rejected2 ? fail2(issue("invalid_value", text2)) : result2;
        })
      });
    }
    default(fallback) {
      const previous = definitionOf(this);
      return create({
        ...previous,
        missing: "parse",
        backend: declare2((value, options) => {
          if (value === MISSING || value === void 0) {
            value = isFunction(fallback) ? assertSync(fallback()) : fallback;
          }
          return decode2(previous.backend, value, options);
        })
      });
    }
    catch(fallback) {
      const previous = definitionOf(this);
      return create({
        ...previous,
        literals: void 0,
        missing: "parse",
        backend: declare2((value, options) => {
          let result2 = field(previous, value, options);
          if (isSuccess2(result2) && result2.success === MISSING)
            result2 = decode2(previous.backend, void 0, options);
          if (isSuccess2(result2)) return result2;
          const context3 = {
            input: value === MISSING ? void 0 : value,
            issues: flatten6(result2.failure, {}, stateOf(options).path)
          };
          const replacement = isFunction(fallback) ? assertSync(fallback(context3)) : fallback;
          return decode2(previous.backend, replacement, options);
        })
      });
    }
    refine(predicate, options = {}) {
      if (!isFunction(predicate))
        throw new SchemaUsageError("A refinement requires a predicate.");
      const text2 = message(options, "The value does not satisfy this check.");
      const path = validatePath(options.path ?? []);
      if (options.rule !== void 0 && typeof options.rule !== "string")
        throw new SchemaUsageError("A rule name must be a string.");
      return this.check((value, context3) => {
        const accepted = assertSync(predicate(value));
        if (typeof accepted !== "boolean")
          throw new SchemaUsageError("A refinement must return a boolean.");
        if (!accepted)
          context3.addIssue({
            message: text2,
            path,
            ...options.rule === void 0 ? {} : { rule: options.rule }
          });
      });
    }
    check(callback3) {
      if (!isFunction(callback3))
        throw new SchemaUsageError("A check requires a callback.");
      return create({
        ...definitionOf(this),
        backend: this.checkedBackend((value, context3) => {
          const result2 = assertSync(callback3(value, context3));
          if (result2 !== void 0)
            throw new SchemaUsageError(
              "A check callback must report issues and return undefined."
            );
          return value;
        })
      });
    }
    transform(callback3) {
      return create({
        ...definitionOf(this),
        backend: this.checkedBackend(callback3)
      });
    }
    preprocess(callback3) {
      const previous = definitionOf(this);
      const preprocessing = create({
        backend: Unknown2,
        missing: "reject",
        metadata: previous.metadata
      }).transform(callback3);
      const preprocessingBackend = definitionOf(preprocessing).backend;
      const backend = declare2((input, options) => {
        const result2 = decode2(
          preprocessingBackend,
          input === MISSING ? void 0 : input,
          options
        );
        return isFailure2(result2) ? result2 : decode2(previous.backend, result2.success, options);
      });
      return create({
        ...previous,
        missing: "parse",
        literals: void 0,
        backend
      });
    }
    pipe(next) {
      const previous = definitionOf(this);
      const target = definitionOf(next);
      return create({
        ...previous,
        missing: /* @__PURE__ */ __name(() => missingOf(previous) === "reject" ? "reject" : "parse", "missing"),
        backend: declare2((value, options) => {
          const result2 = field(previous, value, options);
          return isFailure2(result2) ? result2 : field(target, result2.success, options);
        })
      });
    }
    brand(name) {
      if (typeof name !== "string" || name.length === 0)
        throw new SchemaUsageError("A brand needs a nonempty name.");
      return create(definitionOf(this));
    }
    readonly() {
      return create(definitionOf(this));
    }
    describe(text2) {
      if (typeof text2 !== "string")
        throw new SchemaUsageError("A description must be a string.");
      return create({ ...definitionOf(this), description: text2 });
    }
    meta(metadata) {
      if (!isObject(metadata))
        throw new SchemaUsageError("Metadata must be an object.");
      const previous = definitionOf(this);
      return create({
        ...previous,
        metadata: Object.freeze({ ...previous.metadata, ...metadata })
      });
    }
    constraint(predicate, code, text2) {
      const previous = definitionOf(this);
      const backend = previous.backend.check(
        makeFilter2(
          (value) => predicate(value) || issue(code, text2)
        )
      );
      return { ...previous, backend };
    }
    converted(callback3) {
      const previous = definitionOf(this);
      return {
        ...previous,
        backend: previous.backend.pipe(
          decodeTo2(Unknown2, {
            decode: transform((value) => callback3(value)),
            encode: passthrough2()
          })
        )
      };
    }
    checkedBackend(callback3) {
      if (!isFunction(callback3))
        throw new SchemaUsageError("Expected a synchronous schema callback.");
      return definitionOf(this).backend.pipe(
        decodeTo2(Unknown2, {
          decode: transformEffect((value, options) => {
            const state = stateOf(options);
            const issues = [];
            let active = true;
            const context3 = Object.freeze({
              path: Object.freeze([...state.path]),
              addIssue: /* @__PURE__ */ __name((entry) => {
                if (!active)
                  throw new SchemaUsageError(
                    "A validation context cannot be used after its callback returns."
                  );
                if (!isObject(entry) || typeof entry.message !== "string" || entry.rule !== void 0 && typeof entry.rule !== "string") {
                  throw new SchemaUsageError(
                    "An issue needs a string message and an optional string rule."
                  );
                }
                if (issues.length >= state.maxIssues || state.errors === "first" && issues.length > 0)
                  return;
                issues.push({
                  code: "custom",
                  message: entry.message,
                  path: validatePath(entry.path ?? []),
                  ...entry.rule === void 0 ? {} : { rule: entry.rule }
                });
              }, "addIssue")
            });
            let result2;
            try {
              result2 = assertSync(callback3(value, context3));
            } finally {
              active = false;
            }
            if (result2 === INVALID && issues.length === 0)
              issues.push({
                code: "custom",
                message: "The value could not be transformed.",
                path: []
              });
            return issues.length > 0 ? fail5(fromIssues(issues)) : succeed7(result2);
          }),
          encode: passthrough2()
        })
      );
    }
  };
  var create = /* @__PURE__ */ __name((definition) => {
    const schema = new SchemaImpl(definition);
    Object.freeze(schema);
    return schema;
  }, "create");

  // packages/runtime/src/game/scripting/schema/primitives.ts
  var primitive = /* @__PURE__ */ __name((backend, options) => {
    message(options, "Invalid value.");
    if (options?.message === void 0) return baseDefinition(backend);
    const text2 = options.message;
    return baseDefinition(
      declare2((input, parseOptions) => {
        const result2 = decode2(backend, input, parseOptions);
        return isSuccess2(result2) ? result2 : fail2(issue("invalid_type", text2));
      })
    );
  }, "primitive");
  var StringImpl = class extends SchemaImpl {
    static {
      __name(this, "StringImpl");
    }
    min(length, options) {
      size(length);
      return newString(
        this.constraint(
          (value) => value.length >= length,
          "too_small",
          message(options, `Use at least ${length} characters.`)
        )
      );
    }
    max(length, options) {
      size(length);
      return newString(
        this.constraint(
          (value) => value.length <= length,
          "too_big",
          message(options, `Use at most ${length} characters.`)
        )
      );
    }
    length(length, options) {
      size(length);
      return newString(
        this.constraint(
          (value) => value.length === length,
          "invalid_value",
          message(options, `Use exactly ${length} characters.`)
        )
      );
    }
    regex(pattern, options) {
      if (!(pattern instanceof RegExp))
        throw new SchemaUsageError("Expected a regular expression.");
      const own = new RegExp(pattern.source, pattern.flags);
      return newString(
        this.constraint(
          (value) => {
            own.lastIndex = 0;
            return own.test(value);
          },
          "invalid_format",
          message(options, `Use a value matching ${pattern}.`)
        )
      );
    }
    startsWith(prefix, options) {
      assertString(prefix);
      return newString(
        this.constraint(
          (value) => value.startsWith(prefix),
          "invalid_format",
          message(options, `Start with ${JSON.stringify(prefix)}.`)
        )
      );
    }
    endsWith(suffix, options) {
      assertString(suffix);
      return newString(
        this.constraint(
          (value) => value.endsWith(suffix),
          "invalid_format",
          message(options, `End with ${JSON.stringify(suffix)}.`)
        )
      );
    }
    includes(part, options) {
      assertString(part);
      return newString(
        this.constraint(
          (value) => value.includes(part),
          "invalid_format",
          message(options, `Include ${JSON.stringify(part)}.`)
        )
      );
    }
    trim() {
      return newString(this.converted((value) => value.trim()));
    }
    toLowerCase() {
      return newString(this.converted((value) => value.toLowerCase()));
    }
    toUpperCase() {
      return newString(this.converted((value) => value.toUpperCase()));
    }
    normalize(form = "NFC") {
      if (!["NFC", "NFD", "NFKC", "NFKD"].includes(form))
        throw new SchemaUsageError("Unknown Unicode normalization form.");
      return newString(this.converted((value) => value.normalize(form)));
    }
  };
  var NumberImpl = class extends SchemaImpl {
    static {
      __name(this, "NumberImpl");
    }
    int(options) {
      return newNumber(
        this.constraint(
          Number.isSafeInteger,
          "invalid_value",
          message(options, "Use a safe whole number.")
        )
      );
    }
    min(bound, options) {
      finite2(bound);
      return newNumber(
        this.constraint(
          (value) => value >= bound,
          "too_small",
          message(options, `Use a number of ${bound} or more.`)
        )
      );
    }
    max(bound, options) {
      finite2(bound);
      return newNumber(
        this.constraint(
          (value) => value <= bound,
          "too_big",
          message(options, `Use a number of ${bound} or less.`)
        )
      );
    }
    gt(bound, options) {
      finite2(bound);
      return newNumber(
        this.constraint(
          (value) => value > bound,
          "too_small",
          message(options, `Use a number greater than ${bound}.`)
        )
      );
    }
    lt(bound, options) {
      finite2(bound);
      return newNumber(
        this.constraint(
          (value) => value < bound,
          "too_big",
          message(options, `Use a number less than ${bound}.`)
        )
      );
    }
    multipleOf(divisor, options) {
      finite2(divisor);
      if (divisor <= 0)
        throw new SchemaUsageError("A multiple must be greater than zero.");
      return newNumber(
        this.constraint(
          (value) => remainder(value, divisor) === 0,
          "not_multiple_of",
          message(options, `Use a multiple of ${divisor}.`)
        )
      );
    }
  };
  var DateImpl = class extends SchemaImpl {
    static {
      __name(this, "DateImpl");
    }
    min(bound, options) {
      const time = dateBound(bound);
      return newDate(
        this.constraint(
          (value) => value.getTime() >= time,
          "too_small",
          message(options, `Use a date on or after ${bound.toISOString()}.`)
        )
      );
    }
    max(bound, options) {
      const time = dateBound(bound);
      return newDate(
        this.constraint(
          (value) => value.getTime() <= time,
          "too_big",
          message(options, `Use a date on or before ${bound.toISOString()}.`)
        )
      );
    }
    gt(bound, options) {
      const time = dateBound(bound);
      return newDate(
        this.constraint(
          (value) => value.getTime() > time,
          "too_small",
          message(options, `Use a date after ${bound.toISOString()}.`)
        )
      );
    }
    lt(bound, options) {
      const time = dateBound(bound);
      return newDate(
        this.constraint(
          (value) => value.getTime() < time,
          "too_big",
          message(options, `Use a date before ${bound.toISOString()}.`)
        )
      );
    }
  };
  var assertString = /* @__PURE__ */ __name((value) => {
    if (typeof value !== "string")
      throw new SchemaUsageError("Expected a string constraint.");
  }, "assertString");
  var dateBound = /* @__PURE__ */ __name((value) => {
    if (!(value instanceof Date) || !Number.isFinite(value.getTime()))
      throw new SchemaUsageError("Expected a valid date bound.");
    return value.getTime();
  }, "dateBound");
  var newString = /* @__PURE__ */ __name((definition) => {
    const schema = new StringImpl(definition);
    Object.freeze(schema);
    return schema;
  }, "newString");
  var newNumber = /* @__PURE__ */ __name((definition) => {
    const schema = new NumberImpl(definition);
    Object.freeze(schema);
    return schema;
  }, "newNumber");
  var newDate = /* @__PURE__ */ __name((definition) => {
    const schema = new DateImpl(definition);
    Object.freeze(schema);
    return schema;
  }, "newDate");
  var validDate = Date4.check(
    makeFilter2((value) => Number.isFinite(value.getTime()), {
      expected: "a valid date"
    })
  );
  var string3 = /* @__PURE__ */ __name((options) => newString(primitive(String4, options)), "string");
  var number3 = /* @__PURE__ */ __name((options) => newNumber(primitive(Finite, options)), "number");
  var boolean2 = /* @__PURE__ */ __name((options) => create(primitive(Boolean3, options)), "boolean");
  var date = /* @__PURE__ */ __name((options) => newDate(primitive(validDate, options)), "date");
  var literal = /* @__PURE__ */ __name((value, options) => {
    if (value !== null && value !== void 0 && typeof value !== "string" && typeof value !== "number" && typeof value !== "boolean") {
      throw new SchemaUsageError(
        "A literal must be a string, number, boolean, null, or undefined."
      );
    }
    const backend = value === void 0 ? Undefined2 : value === null ? Null2 : Literal2(value);
    return create({
      ...primitive(backend, options),
      literals: Object.freeze([value])
    });
  }, "literal");
  var EnumImpl = class extends SchemaImpl {
    static {
      __name(this, "EnumImpl");
    }
    values;
    constructor(values, options) {
      if (!Array.isArray(values) || values.length === 0 || !values.every(
        (value) => typeof value === "string" || typeof value === "number"
      )) {
        throw new SchemaUsageError(
          "An enum needs a nonempty array of strings or numbers."
        );
      }
      const copy3 = Object.freeze([...new Set(values)]);
      super({ ...primitive(Literals(copy3), options), literals: copy3 });
      this.values = copy3;
      Object.freeze(this);
    }
  };
  var custom = /* @__PURE__ */ __name((predicate, options) => {
    if (!isFunction(predicate))
      throw new SchemaUsageError("A custom schema requires a predicate.");
    const text2 = message(options, "The value does not match the required type.");
    return create(
      baseDefinition(
        declare(
          (value) => {
            const accepted = assertSync(predicate(value));
            if (typeof accepted !== "boolean")
              throw new SchemaUsageError(
                "A custom predicate must return a boolean."
              );
            return accepted;
          },
          { title: text2 }
        )
      )
    );
  }, "custom");
  var decimal = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
  var decimalInput = Union2([Finite, String4]);
  var coercion = /* @__PURE__ */ __name((source, convert, options, expected) => {
    const text2 = message(options, expected);
    return baseDefinition(
      declare2((input, parseOptions) => {
        const accepted = decode2(source, input, parseOptions);
        if (isFailure2(accepted))
          return fail2(issue("invalid_type", text2));
        const value = convert(accepted.success);
        return value === rejected ? fail2(issue("invalid_value", text2)) : succeed2(value);
      })
    );
  }, "coercion");
  var rejected = /* @__PURE__ */ Symbol("invalid-coercion");
  var parseDate = /* @__PURE__ */ __name((input) => {
    if (input instanceof Date)
      return Number.isFinite(input.getTime()) ? input : rejected;
    if (typeof input === "number") {
      const value2 = new Date(input);
      return Number.isFinite(value2.getTime()) ? value2 : rejected;
    }
    if (typeof input !== "string") return rejected;
    const match6 = /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}):(\d{2})(?:\.\d{1,3})?(?:Z|([+-])(\d{2}):(\d{2})))?$/.exec(
      input
    );
    if (match6 === null) return rejected;
    const year = Number(match6[1]);
    const month = Number(match6[2]);
    const day = Number(match6[3]);
    const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
    const monthDays = [
      31,
      leap ? 29 : 28,
      31,
      30,
      31,
      30,
      31,
      31,
      30,
      31,
      30,
      31
    ];
    if (month < 1 || month > 12 || day < 1 || day > (monthDays[month - 1] ?? 0) || Number(match6[4] ?? 0) > 23 || Number(match6[5] ?? 0) > 59 || Number(match6[6] ?? 0) > 59 || Number(match6[8] ?? 0) > 23 || Number(match6[9] ?? 0) > 59)
      return rejected;
    const value = new Date(input);
    return Number.isFinite(value.getTime()) ? value : rejected;
  }, "parseDate");
  var coerce = Object.freeze({
    string: /* @__PURE__ */ __name((options) => newString(
      coercion(
        Union2([String4, Finite, Boolean3]),
        String,
        options,
        "Use a string, finite number, or boolean."
      )
    ), "string"),
    number: /* @__PURE__ */ __name((options) => newNumber(
      coercion(
        decimalInput,
        (value) => {
          if (typeof value === "number") return value;
          if (typeof value !== "string" || !decimal.test(value.trim()))
            return rejected;
          const parsed = Number(value.trim());
          return Number.isFinite(parsed) ? parsed : rejected;
        },
        options,
        "Use a finite number or decimal numeric string."
      )
    ), "number"),
    boolean: /* @__PURE__ */ __name((options) => create(
      coercion(
        Literals([true, false, 0, 1, "0", "1", "false", "true"]),
        (value) => value === true || value === 1 || value === "1" || value === "true",
        options,
        "Use true, false, 0, or 1."
      )
    ), "boolean"),
    date: /* @__PURE__ */ __name((options) => newDate(
      coercion(
        Union2([Date4, Finite, String4]),
        parseDate,
        options,
        "Use a valid date, timestamp, or ISO date string."
      )
    ), "date")
  });

  // packages/runtime/src/game/scripting/schema/collections.ts
  var plainObject = declare(
    (value) => isObject(value) && (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null),
    { title: "a plain object" }
  );
  var arrayValue = declare(Array.isArray, {
    title: "an array"
  });
  var objectConfigs = /* @__PURE__ */ new WeakMap();
  var ObjectImpl = class _ObjectImpl extends SchemaImpl {
    static {
      __name(this, "ObjectImpl");
    }
    shape;
    constructor(fields, policy = "strip", options) {
      if (!isObject(fields))
        throw new SchemaUsageError("An object schema needs a field map.");
      const shape = Object.freeze({ ...fields });
      const entries = Object.entries(shape).map(
        ([key, schema]) => [key, definitionOf(schema)]
      );
      const knownKeys = new Set(entries.map(([key]) => key));
      const text2 = message(options, "Expected a plain object.");
      super(
        baseDefinition(
          declare2((input, parseOptions) => {
            const accepted = decode2(plainObject, input, parseOptions);
            if (isFailure2(accepted))
              return fail2(issue("invalid_type", text2));
            const source = accepted.success;
            return container(source, parseOptions, () => {
              const output = {};
              const issues = [];
              for (const [key, definition] of entries) {
                const value = Object.hasOwn(source, key) ? source[key] : MISSING;
                const result2 = child(definition, value, key, parseOptions);
                if (isFailure2(result2))
                  collect(issues, result2.failure, parseOptions);
                else if (result2.success !== MISSING)
                  writeProperty(output, key, result2.success);
                if (!canContinue(issues, parseOptions)) break;
              }
              if (policy !== "strip" && canContinue(issues, parseOptions)) {
                for (const key of Object.keys(source)) {
                  if (knownKeys.has(key)) continue;
                  if (policy === "strict") {
                    issues.push({
                      code: "unrecognized_key",
                      path: [key],
                      message: "This field is not allowed."
                    });
                  } else if (policy === "passthrough") {
                    writeProperty(output, key, source[key]);
                  } else {
                    const result2 = child(policy, source[key], key, parseOptions);
                    if (isFailure2(result2))
                      collect(issues, result2.failure, parseOptions);
                    else writeProperty(output, key, result2.success);
                  }
                  if (!canContinue(issues, parseOptions)) break;
                }
              }
              return issues.length > 0 ? fail2(fromIssues(issues)) : succeed2(output);
            });
          })
        )
      );
      this.shape = shape;
      objectConfigs.set(this, {
        policy,
        options: options === void 0 ? void 0 : Object.freeze({ ...options })
      });
      Object.freeze(this);
    }
    extend(fields) {
      if (!isObject(fields))
        throw new SchemaUsageError(
          "Expected fields to extend the object schema."
        );
      return this.rebuild({ ...this.shape, ...fields });
    }
    pick(keys) {
      const selected = this.keys(keys);
      return this.rebuild(
        Object.fromEntries(
          Object.entries(this.shape).filter(([key]) => selected.has(key))
        )
      );
    }
    omit(keys) {
      const omitted = this.keys(keys);
      return this.rebuild(
        Object.fromEntries(
          Object.entries(this.shape).filter(([key]) => !omitted.has(key))
        )
      );
    }
    partial(keys = Object.keys(this.shape)) {
      const selected = this.keys(keys);
      return this.rebuild(
        Object.fromEntries(
          Object.entries(this.shape).map(([key, schema]) => [
            key,
            selected.has(key) ? new SchemaImpl(definitionOf(schema)).optional() : schema
          ])
        )
      );
    }
    required(keys = Object.keys(this.shape)) {
      const selected = this.keys(keys);
      return this.rebuild(
        Object.fromEntries(
          Object.entries(this.shape).map(([key, schema]) => [
            key,
            selected.has(key) ? new SchemaImpl(definitionOf(schema)).required() : schema
          ])
        )
      );
    }
    strip() {
      return this.rebuild(this.shape, "strip");
    }
    strict() {
      return this.rebuild(this.shape, "strict");
    }
    passthrough() {
      return this.rebuild(this.shape, "passthrough");
    }
    catchall(schema) {
      return this.rebuild(this.shape, definitionOf(schema));
    }
    keyof() {
      return new EnumImpl(Object.keys(this.shape));
    }
    keys(keys) {
      if (!Array.isArray(keys) || !keys.every(
        (key) => typeof key === "string" && Object.hasOwn(this.shape, key)
      )) {
        throw new SchemaUsageError("Select keys declared by this object schema.");
      }
      return new Set(keys);
    }
    rebuild(fields, policy) {
      const config = objectConfigs.get(this);
      if (config === void 0)
        throw new SchemaUsageError("Expected an object schema.");
      return new _ObjectImpl(fields, policy ?? config.policy, config.options);
    }
  };
  var ArrayImpl = class _ArrayImpl extends SchemaImpl {
    static {
      __name(this, "ArrayImpl");
    }
    element;
    constructor(element, options, definition) {
      const childDefinition = definitionOf(element);
      const text2 = message(options, "Expected an array.");
      super(
        definition ?? baseDefinition(
          declare2((input, parseOptions) => {
            const accepted = decode2(arrayValue, input, parseOptions);
            if (isFailure2(accepted))
              return fail2(issue("invalid_type", text2));
            const source = accepted.success;
            return container(source, parseOptions, () => {
              const output = [];
              const issues = [];
              for (let index = 0; index < source.length; index += 1) {
                const result2 = child(
                  childDefinition,
                  source[index],
                  index,
                  parseOptions
                );
                if (isFailure2(result2))
                  collect(issues, result2.failure, parseOptions);
                else output.push(result2.success);
                if (!canContinue(issues, parseOptions)) break;
              }
              return issues.length > 0 ? fail2(fromIssues(issues)) : succeed2(output);
            });
          })
        )
      );
      this.element = element;
      Object.freeze(this);
    }
    min(length, options) {
      size(length);
      return new _ArrayImpl(
        this.element,
        void 0,
        this.constraint(
          (value) => value.length >= length,
          "too_small",
          message(options, `Select at least ${length} entries.`)
        )
      );
    }
    max(length, options) {
      size(length);
      return new _ArrayImpl(
        this.element,
        void 0,
        this.constraint(
          (value) => value.length <= length,
          "too_big",
          message(options, `Use at most ${length} entries.`)
        )
      );
    }
    length(length, options) {
      size(length);
      return new _ArrayImpl(
        this.element,
        void 0,
        this.constraint(
          (value) => value.length === length,
          "invalid_value",
          message(options, `Use exactly ${length} entries.`)
        )
      );
    }
    unique(selector, options) {
      if (selector !== void 0 && !isFunction(selector))
        throw new SchemaUsageError("A uniqueness selector must be a function.");
      return new _ArrayImpl(
        this.element,
        void 0,
        this.constraint(
          (values) => {
            const keys = /* @__PURE__ */ new Set();
            for (const value of values) {
              const key = selector === void 0 ? value : assertSync(selector(value));
              if (keys.has(key)) return false;
              keys.add(key);
            }
            return true;
          },
          "duplicate_value",
          message(options, "Each entry must be unique.")
        )
      );
    }
  };
  var tupleConfigs = /* @__PURE__ */ new WeakMap();
  var TupleImpl = class _TupleImpl extends SchemaImpl {
    static {
      __name(this, "TupleImpl");
    }
    items;
    constructor(items, rest, options) {
      if (!Array.isArray(items))
        throw new SchemaUsageError("A tuple needs an array of schemas.");
      const definitions2 = items.map(definitionOf);
      const validateOrder = /* @__PURE__ */ __name(() => {
        let optional2 = false;
        for (const definition of definitions2) {
          const missing2 = missingOf(definition);
          if (optional2 && missing2 === "reject")
            throw new SchemaUsageError(
              "Optional tuple elements must be trailing."
            );
          optional2 ||= missing2 !== "reject";
        }
      }, "validateOrder");
      let orderValidated = definitions2.every(
        (definition) => typeof definition.missing !== "function"
      );
      if (orderValidated) validateOrder();
      const tail = rest === void 0 ? void 0 : definitionOf(rest);
      const text2 = message(options, "Expected an array tuple.");
      super(
        baseDefinition(
          declare2((input, parseOptions) => {
            if (!orderValidated) {
              validateOrder();
              orderValidated = true;
            }
            const accepted = decode2(arrayValue, input, parseOptions);
            if (isFailure2(accepted))
              return fail2(issue("invalid_type", text2));
            const source = accepted.success;
            return container(source, parseOptions, () => {
              const output = [];
              const issues = [];
              for (let index = 0; index < Math.max(source.length, definitions2.length); index += 1) {
                const definition = definitions2[index] ?? tail;
                if (definition === void 0) {
                  issues.push({
                    code: "unrecognized_key",
                    path: [index],
                    message: "This tuple has an extra element."
                  });
                } else {
                  const value = Object.hasOwn(source, index) ? source[index] : MISSING;
                  const result2 = child(definition, value, index, parseOptions);
                  if (isFailure2(result2))
                    collect(issues, result2.failure, parseOptions);
                  else if (result2.success !== MISSING)
                    output[index] = result2.success;
                }
                if (!canContinue(issues, parseOptions)) break;
              }
              return issues.length > 0 ? fail2(fromIssues(issues)) : succeed2(output);
            });
          })
        )
      );
      this.items = Object.freeze([...items]);
      tupleConfigs.set(this, { options });
      Object.freeze(this);
    }
    rest(schema) {
      return new _TupleImpl(this.items, schema, tupleConfigs.get(this)?.options);
    }
  };
  var RecordImpl = class _RecordImpl extends SchemaImpl {
    static {
      __name(this, "RecordImpl");
    }
    key;
    value;
    constructor(key, value, options, definition) {
      const keyDefinition = definitionOf(key);
      const valueDefinition = definitionOf(value);
      const text2 = message(options, "Expected a plain object record.");
      super(
        definition ?? baseDefinition(
          declare2((input, parseOptions) => {
            const accepted = decode2(plainObject, input, parseOptions);
            if (isFailure2(accepted))
              return fail2(issue("invalid_type", text2));
            const source = accepted.success;
            return container(source, parseOptions, () => {
              const output = {};
              const issues = [];
              for (const key2 of Object.keys(source)) {
                const parsedKey = child(keyDefinition, key2, key2, parseOptions);
                if (isFailure2(parsedKey)) {
                  const keyIssues = [];
                  collect(keyIssues, parsedKey.failure, parseOptions);
                  issues.push(
                    ...keyIssues.map(
                      (entry) => ({ ...entry, code: "invalid_key" })
                    )
                  );
                } else if (typeof parsedKey.success !== "string") {
                  throw new SchemaUsageError(
                    "Record keys must decode to strings."
                  );
                } else if (Object.hasOwn(output, parsedKey.success)) {
                  issues.push({
                    code: "duplicate_key",
                    path: [key2],
                    message: "This key duplicates another decoded key."
                  });
                } else {
                  const result2 = child(
                    valueDefinition,
                    source[key2],
                    key2,
                    parseOptions
                  );
                  if (isFailure2(result2))
                    collect(issues, result2.failure, parseOptions);
                  else writeProperty(output, parsedKey.success, result2.success);
                }
                if (!canContinue(issues, parseOptions)) break;
              }
              return issues.length > 0 ? fail2(fromIssues(issues)) : succeed2(output);
            });
          })
        )
      );
      this.key = key;
      this.value = value;
      Object.freeze(this);
    }
    min(bound, options) {
      size(bound, "size");
      return new _RecordImpl(
        this.key,
        this.value,
        void 0,
        this.constraint(
          (value) => Object.keys(value).length >= bound,
          "too_small",
          message(options, `Use at least ${bound} entries.`)
        )
      );
    }
    max(bound, options) {
      size(bound, "size");
      return new _RecordImpl(
        this.key,
        this.value,
        void 0,
        this.constraint(
          (value) => Object.keys(value).length <= bound,
          "too_big",
          message(options, `Use at most ${bound} entries.`)
        )
      );
    }
    size(bound, options) {
      size(bound, "size");
      return new _RecordImpl(
        this.key,
        this.value,
        void 0,
        this.constraint(
          (value) => Object.keys(value).length === bound,
          "invalid_value",
          message(options, `Use exactly ${bound} entries.`)
        )
      );
    }
  };
  var record2 = /* @__PURE__ */ __name((first, second, options) => {
    if (isSchema2(second)) {
      return new RecordImpl(first, second, options);
    }
    return new RecordImpl(string3(), first, second);
  }, "record");

  // packages/runtime/src/game/scripting/schema/index.ts
  var union2 = /* @__PURE__ */ __name((members, options) => {
    if (!Array.isArray(members) || members.length === 0)
      throw new SchemaUsageError("A union needs at least one schema.");
    const definitions2 = members.map(definitionOf);
    const text2 = message(
      options,
      "Expected one of the allowed values or shapes."
    );
    return create(
      baseDefinition(
        declare2((input, parseOptions) => {
          const failures = [];
          for (const definition of definitions2) {
            const result2 = decode2(definition.backend, input, parseOptions);
            if (isSuccess2(result2)) return result2;
            failures.push(result2.failure);
          }
          return fail2(
            fromIssues([
              {
                code: "invalid_union",
                path: [],
                message: text2,
                branches: failures.map(
                  (failure2) => flatten6(failure2, {
                    maxIssues: stateOf(parseOptions).maxIssues
                  })
                )
              }
            ])
          );
        })
      )
    );
  }, "union");
  var discriminatedUnion = /* @__PURE__ */ __name((key, members, options) => {
    if (typeof key !== "string" || !Array.isArray(members) || members.length === 0)
      throw new SchemaUsageError(
        "A discriminated union needs a key and object schemas."
      );
    const branches = /* @__PURE__ */ new Map();
    for (const member of members) {
      if (!(member instanceof ObjectImpl))
        throw new SchemaUsageError(
          "Discriminated union members must be object schemas."
        );
      const discriminator = definitionOf(member.shape[key]);
      if (discriminator.missing !== "reject" || discriminator.literals === void 0) {
        throw new SchemaUsageError(
          "A discriminator must be a required literal or enum."
        );
      }
      for (const value of discriminator.literals) {
        if (branches.has(value))
          throw new SchemaUsageError("Discriminator values must be unique.");
        branches.set(value, definitionOf(member).backend);
      }
    }
    const text2 = message(options, `Use a recognized ${key} value.`);
    return create(
      baseDefinition(
        declare2((input, parseOptions) => {
          if (!isObject(input) || !Object.hasOwn(input, key))
            return fail2(issue("invalid_union", text2, [key]));
          const backend = branches.get(input[key]);
          return backend === void 0 ? fail2(issue("invalid_union", text2, [key])) : decode2(backend, input, parseOptions);
        })
      )
    );
  }, "discriminatedUnion");
  var lazy = /* @__PURE__ */ __name((factory) => {
    if (!isFunction(factory))
      throw new SchemaUsageError("A lazy schema needs a factory.");
    let resolved;
    let resolving = false;
    let resolvingMissing = false;
    const resolve3 = /* @__PURE__ */ __name(() => {
      if (resolved === void 0) {
        if (resolving)
          throw new SchemaUsageError(
            "A lazy factory cannot parse its own schema while resolving."
          );
        resolving = true;
        try {
          resolved = definitionOf(assertSync(factory()));
        } finally {
          resolving = false;
        }
      }
      return resolved;
    }, "resolve");
    const active = /* @__PURE__ */ new WeakMap();
    const backend = declare2((input, options) => {
      const target = resolve3();
      const state = stateOf(options);
      let inputs = active.get(state);
      if (inputs === void 0) {
        inputs = /* @__PURE__ */ new Set();
        active.set(state, inputs);
      }
      if (inputs.has(input))
        return fail2(
          issue(
            "cyclic_reference",
            "A recursive schema revisited the same value."
          )
        );
      inputs.add(input);
      try {
        return decode2(target.backend, input, options);
      } finally {
        inputs.delete(input);
      }
    });
    return create({
      ...baseDefinition(backend),
      missing: /* @__PURE__ */ __name(() => {
        if (resolving || resolvingMissing) return "reject";
        resolvingMissing = true;
        try {
          return missingOf(resolve3());
        } finally {
          resolvingMissing = false;
        }
      }, "missing")
    });
  }, "lazy");
  var json = /* @__PURE__ */ __name((schema, options) => {
    const target = definitionOf(schema);
    const text2 = message(options, "Use valid JSON text.");
    return create(
      baseDefinition(
        declare2((input, parseOptions) => {
          const accepted = decode2(String4, input, parseOptions);
          if (isFailure2(accepted))
            return fail2(issue("invalid_type", "Expected JSON text."));
          let parsed;
          try {
            parsed = JSON.parse(accepted.success);
          } catch (error) {
            if (!(error instanceof SyntaxError)) throw error;
            return fail2(issue("invalid_json", text2));
          }
          return decode2(target.backend, parsed, parseOptions);
        })
      )
    );
  }, "json");
  var jsonValue = /* @__PURE__ */ __name(() => {
    const scalar = Union2([
      Null2,
      Boolean3,
      Finite,
      String4
    ]);
    const value = lazy(
      () => create(
        baseDefinition(
          declare2((input, options) => {
            const backend = Array.isArray(input) ? arrayBackend : isObject(input) ? recordBackend : scalar;
            return decode2(backend, input, options);
          })
        )
      )
    );
    const arrayBackend = definitionOf(new ArrayImpl(value)).backend;
    const recordBackend = definitionOf(new RecordImpl(string3(), value)).backend;
    return value;
  }, "jsonValue");
  var instanceOf2 = /* @__PURE__ */ __name((constructor, options) => {
    if (!isFunction(constructor))
      throw new SchemaUsageError("Expected a class constructor.");
    return custom((value) => value instanceof constructor, options);
  }, "instanceOf");
  for (const implementation of [
    SchemaImpl,
    StringImpl,
    NumberImpl,
    DateImpl,
    ObjectImpl,
    ArrayImpl,
    RecordImpl,
    TupleImpl,
    EnumImpl
  ]) {
    Object.freeze(implementation.prototype);
  }
  var scriptSchema = Object.freeze({
    string: string3,
    number: number3,
    boolean: boolean2,
    date,
    literal,
    custom,
    instanceOf: instanceOf2,
    union: union2,
    discriminatedUnion,
    lazy,
    json,
    jsonValue,
    null: /* @__PURE__ */ __name((options) => create(primitive(Null2, options)), "null"),
    undefined: /* @__PURE__ */ __name((options) => create(primitive(Undefined2, options)), "undefined"),
    unknown: /* @__PURE__ */ __name(() => create(baseDefinition(Unknown2)), "unknown"),
    never: /* @__PURE__ */ __name((options) => create(primitive(Never2, options)), "never"),
    enum: /* @__PURE__ */ __name((values, options) => new EnumImpl(values, options), "enum"),
    object: /* @__PURE__ */ __name((fields, options) => new ObjectImpl(fields, "strip", options), "object"),
    array: /* @__PURE__ */ __name((element, options) => new ArrayImpl(element, options), "array"),
    tuple: /* @__PURE__ */ __name((elements, options) => new TupleImpl(elements, void 0, options), "tuple"),
    record: record2,
    coerce,
    INVALID,
    isSchema: isSchema2,
    ValidationError,
    SchemaUsageError,
    formatIssues,
    formatPath: formatPath2
  });

  // packages/runtime/src/game/scripting/ScriptBuiltinModules.ts
  var makeScriptAutoReloginApi = /* @__PURE__ */ __name((api2) => Object.freeze({
    getDelayMs: api2.getDelayMs,
    getServer: api2.getServer,
    getState: api2.getState,
    isEnabled: api2.isEnabled,
    setDelay: api2.setDelay,
    setEnabled: api2.setEnabled,
    setServer: api2.setServer
  }), "makeScriptAutoReloginApi");
  var makeScriptAutoZoneApi = /* @__PURE__ */ __name((api2) => Object.freeze({
    getMap: api2.getMap,
    getState: api2.getState,
    isEnabled: api2.isEnabled,
    setEnabled: api2.setEnabled,
    setMap: api2.setMap
  }), "makeScriptAutoZoneApi");
  var makeScriptBuiltinModules = /* @__PURE__ */ __name((options) => {
    const army = makeScriptArmyApi(
      options.services.army,
      options.scope,
      options.failCause
    );
    const events = makeScriptEventsApi(
      options.services.events,
      options.scope,
      options.failCause
    );
    const environment = makeScriptEnvironmentApi(options.services.environment);
    const packet = makeScriptPacketApi(
      options.services.packet,
      options.scope,
      options.failCause
    );
    const { player, players } = makeScriptPlayerApis(
      options.services.player,
      options.services.players,
      { policy: options.roomPolicy, map: options.services.map }
    );
    const settings = makeScriptSettingsApi(options.services.settings);
    const shop = makeScriptShopApi(
      {
        inventory: options.services.inventory,
        player: {
          getLevel: player.getLevel,
          isMember: player.isMember,
          joinMap: player.joinMap
        },
        shops: options.services.shops,
        wait: options.services.wait
      },
      options.bridge
    );
    const combat = makeScriptCombatApi(options.services.combat);
    const publicServices = makeScriptPublicServices(options.services);
    const api2 = Object.freeze({
      army,
      auth: publicServices.auth,
      bank: publicServices.bank,
      combat,
      drops: publicServices.drops,
      environment,
      events,
      house: publicServices.house,
      inventory: publicServices.inventory,
      map: publicServices.map,
      monsters: publicServices.monsters,
      packet,
      player,
      players,
      quests: publicServices.quests,
      settings,
      shop,
      tempInventory: publicServices.tempInventory,
      wait: publicServices.wait
    });
    return Object.freeze({
      effect: scriptEffectStd,
      "lucent/api": api2,
      "lucent/autorelogin": makeScriptAutoReloginApi(options.autoRelogin),
      "lucent/autozone": makeScriptAutoZoneApi(options.autoZone),
      "lucent/filesystem": options.fileSystem,
      "lucent/http": options.http,
      "lucent/schema": scriptSchema,
      "lucent/script": options.script
    });
  }, "makeScriptBuiltinModules");

  // packages/runtime/src/game/scripting/ScriptFileSystem.ts
  var makeError = /* @__PURE__ */ __name((operation, reason, path) => new FileSystemError({
    operation,
    reason,
    ...path === void 0 ? {} : { path }
  }), "makeError");
  var isJson2 = is2(Json2);
  var tooLarge = /* @__PURE__ */ __name((text2) => {
    let bytes = 0;
    for (let index = 0; index < text2.length; index++) {
      const code = text2.charCodeAt(index);
      if (code < 128) bytes++;
      else if (code < 2048) bytes += 2;
      else if (code >= 55296 && code <= 56319 && index + 1 < text2.length && text2.charCodeAt(index + 1) >= 56320 && text2.charCodeAt(index + 1) <= 57343) {
        bytes += 4;
        index++;
      } else bytes += 3;
      if (bytes > SCRIPT_FILE_SYSTEM_MAX_BYTES) return true;
    }
    return false;
  }, "tooLarge");
  var makeScriptFileSystemApi = /* @__PURE__ */ __name((bridge, scope3) => {
    const request = /* @__PURE__ */ __name((operation, path, invoke, validate2) => suspend2(() => {
      if (scope3.signal.aborted) {
        return fail5(makeError(operation, "session-closed", path));
      }
      const validationError = validate2?.();
      if (validationError !== void 0) {
        return fail5(validationError);
      }
      return tryPromise2({
        try: invoke,
        catch: /* @__PURE__ */ __name(() => makeError(operation, "unavailable", path), "catch")
      }).pipe(
        flatMap3(
          (result2) => result2.ok ? succeed7(result2.value) : fail5(
            makeError(
              result2.error.operation,
              result2.error.reason,
              result2.error.path
            )
          )
        )
      );
    }), "request");
    const api2 = {
      FileSystemError,
      exists: /* @__PURE__ */ __name((path) => request("exists", path, (signal) => bridge.exists(path, signal)), "exists"),
      list: /* @__PURE__ */ __name((path) => request("list", path, (signal) => bridge.list(path, signal)).pipe(
        map5(
          (entries) => Object.freeze(entries.map((entry) => Object.freeze({ ...entry })))
        )
      ), "list"),
      readJson: /* @__PURE__ */ __name((path) => request("read-json", path, (signal) => bridge.readJson(path, signal)), "readJson"),
      readText: /* @__PURE__ */ __name((path) => request("read-text", path, (signal) => bridge.readText(path, signal)), "readText"),
      remove: /* @__PURE__ */ __name((path) => request("remove", path, (signal) => bridge.remove(path, signal)), "remove"),
      writeJson: /* @__PURE__ */ __name((path, value) => request(
        "write-json",
        path,
        (signal) => bridge.writeJson(path, value, signal),
        () => {
          if (!isJson2(value)) {
            return makeError("write-json", "not-json-serializable", path);
          }
          const source = `${JSON.stringify(value, null, 2)}
`;
          return tooLarge(source) ? makeError("write-json", "too-large", path) : void 0;
        }
      ), "writeJson"),
      writeText: /* @__PURE__ */ __name((path, contents) => request(
        "write-text",
        path,
        (signal) => bridge.writeText(path, contents, signal),
        () => tooLarge(contents) ? makeError("write-text", "too-large", path) : void 0
      ), "writeText")
    };
    return Object.freeze(api2);
  }, "makeScriptFileSystemApi");

  // packages/runtime/src/game/scripting/ScriptServices.ts
  var ScriptServices = class extends Service()("lucent/game/scripting/ScriptServices") {
    static {
      __name(this, "ScriptServices");
    }
  };

  // packages/runtime/src/game/scripting/api/Services.ts
  var makeScriptRuntimeServices = /* @__PURE__ */ __name((api2, army, environment) => ({ ...api2, army, environment }), "makeScriptRuntimeServices");

  // packages/runtime/src/shared/logFailure.ts
  var logFailure = /* @__PURE__ */ __name((message2, cause, options) => {
    if (hasInterruptsOnly2(cause)) return void_3;
    if (hasDies2(cause)) return logError(message2, cause);
    switch (options?.failure ?? "debug") {
      case "error":
        return logError(message2, cause);
      case "warning":
        return logWarning(message2, cause);
      case "debug":
        return logDebug(message2, cause);
    }
  }, "logFailure");

  // packages/runtime/src/game/scripting/safeStartStop.ts
  var maximumSafeMoveAttempts = 3;
  var respawnTimeout = "15 seconds";
  var returnInfoPath = "world.returnInfo";
  var SafeMoveTimeout = class extends TaggedError2("SafeMoveTimeout") {
    static {
      __name(this, "SafeMoveTimeout");
    }
  };
  var safeMoveRetrySchedule = exponential("1 second").pipe(
    upTo({ times: maximumSafeMoveAttempts - 1 })
  );
  var logMoveFailure = /* @__PURE__ */ __name((phase) => catchCause2(
    (cause) => logFailure("Safe start/stop move failed", cause).pipe(
      annotateLogs({ component: "scripting", data: { phase } })
    )
  ), "logMoveFailure");
  var retrySafeMove = /* @__PURE__ */ __name((attempt) => attempt.pipe(
    flatMap3(
      (result2) => result2 === "retry" ? fail5(new SafeMoveTimeout()) : succeed7(result2)
    ),
    retry2(safeMoveRetrySchedule),
    catchTag2(
      "SafeMoveTimeout",
      () => succeed7("timed-out")
    )
  ), "retrySafeMove");
  var runWithSafeStartStop = /* @__PURE__ */ __name((main, isEnabled, moveToSafeDestination) => {
    const moveIfEnabled = /* @__PURE__ */ __name((phase) => isEnabled.pipe(
      flatMap3(
        (enabled) => enabled ? moveToSafeDestination(phase) : void_3
      )
    ), "moveIfEnabled");
    return gen2(function* () {
      yield* moveIfEnabled("before");
      return yield* main;
    }).pipe(ensuring2(moveIfEnabled("after")));
  }, "runWithSafeStartStop");
  var makeMoveToSafeDestination = /* @__PURE__ */ __name((services) => {
    const { auth, bridge, combat, house, map: map8, packet, player, roomPolicy, wait } = services;
    const isInOwnHouse = bridge.invokeJson("flash.callGameFunction0", ["world.isMyHouse"], Boolean3).pipe(map5(getOrElse(() => false)));
    const setReturnInfo = /* @__PURE__ */ __name((value) => bridge.invoke("flash.setGameObject", [returnInfoPath, value], Void2).pipe(map5(isSome2)), "setReturnInfo");
    const moveToSafeDestination = fn2("ScriptRunner.moveToSafeDestination")(
      function* () {
        if (!(yield* auth.isLoggedIn())) {
          return;
        }
        const username = (yield* auth.getUsername()).trim();
        if (username === "") {
          return;
        }
        const hasEquippedHouse = (yield* house.getAll()).some(
          (item) => item.category === "House" && item.equipped
        );
        let destination;
        if (hasEquippedHouse) {
          const inOwnHouse = yield* isInOwnHouse;
          if (!(yield* auth.isLoggedIn())) {
            return;
          }
          if (inOwnHouse) return;
          destination = { kind: "house" };
        } else {
          const policy = yield* roomPolicy;
          const [mapName, currentRoomNumber] = yield* all2([
            map8.getName(),
            map8.getRoomNumber()
          ]);
          const inBuyhouse = mapName.localeCompare("buyhouse", void 0, {
            sensitivity: "accent"
          }) === 0;
          if (inBuyhouse && roomPolicyAcceptsRoom(policy, currentRoomNumber)) {
            return;
          }
          destination = {
            kind: "buyhouse",
            target: yield* applyRoomPolicy("buyhouse", policy)
          };
        }
        const alive = yield* player.isAlive();
        if (!(yield* auth.isLoggedIn())) {
          return;
        }
        if (!alive) {
          const respawn = yield* wait.untilSome(
            all2([auth.isLoggedIn(), player.isAlive()]).pipe(
              map5(
                ([loggedIn, alive2]) => !loggedIn ? some2("aborted") : alive2 ? some2("ready") : none2()
              )
            ),
            { timeout: respawnTimeout }
          );
          if (respawn === "aborted") {
            return;
          }
          if (respawn !== "ready") {
            return;
          }
        }
        const exitedCombat = yield* combat.exit();
        if (!(yield* auth.isLoggedIn())) {
          return;
        }
        if (!exitedCombat) {
          return;
        }
        yield* sleep2("1 second");
        if (!(yield* auth.isLoggedIn())) {
          return;
        }
        const move = retrySafeMove(
          destination.kind === "house" ? gen2(function* () {
            if (!(yield* auth.isLoggedIn())) return "aborted";
            if (!(yield* packet.sendToServer(`%xt%zm%house%1%${username}%`))) {
              return "retry";
            }
            const outcome2 = yield* wait.untilSome(
              all2([
                auth.isLoggedIn(),
                isInOwnHouse,
                player.isReady()
              ]).pipe(
                map5(
                  ([loggedIn, ownHouse, ready]) => !loggedIn ? some2("aborted") : ownHouse && ready ? some2("moved") : none2()
                )
              ),
              { timeout: "5 seconds" }
            );
            return outcome2 ?? "retry";
          }) : gen2(function* () {
            if (!(yield* auth.isLoggedIn())) return "aborted";
            const joined = yield* player.joinMap(destination.target);
            if (!(yield* auth.isLoggedIn())) return "aborted";
            if (!joined) return "retry";
            const outcome2 = yield* wait.untilSome(
              all2([auth.isLoggedIn(), player.isReady()]).pipe(
                map5(
                  ([loggedIn, ready]) => !loggedIn ? some2("aborted") : ready ? some2("moved") : none2()
                )
              ),
              { timeout: "5 seconds" }
            );
            return outcome2 ?? "retry";
          })
        );
        const returnInfoIsNull = yield* bridge.invoke(
          "flash.isNull",
          [returnInfoPath],
          Boolean3
        );
        if (getOrElse(returnInfoIsNull, () => false))
          return yield* asVoid2(move);
        const returnInfo = yield* bridge.invokeJson(
          "flash.getGameObject",
          [returnInfoPath],
          Unknown2
        );
        if (isNone2(returnInfo) || !(yield* setReturnInfo(null))) {
          return;
        }
        yield* move.pipe(
          ensuring2(setReturnInfo(returnInfo.value).pipe(asVoid2))
        );
      }
    );
    return (phase) => moveToSafeDestination().pipe(logMoveFailure(phase));
  }, "makeMoveToSafeDestination");

  // packages/runtime/src/game/scripting/ScriptRunner.ts
  var terminalFinalization = /* @__PURE__ */ __name((outcome2) => ({ outcome: outcome2, status: outcome2.status }), "terminalFinalization");
  var ScriptRunner = class extends Service()("lucent/game/scripting/ScriptRunner") {
    static {
      __name(this, "ScriptRunner");
    }
  };
  var nowIso = /* @__PURE__ */ __name(() => (/* @__PURE__ */ new Date()).toISOString(), "nowIso");
  var snapshotStatus = /* @__PURE__ */ __name((status) => ({
    ...status
  }), "snapshotStatus");
  var makeRunHandle = /* @__PURE__ */ __name((initialStatus, terminal) => ({
    initialStatus: snapshotStatus(initialStatus),
    terminal: _await(terminal)
  }), "makeRunHandle");
  var externalStopOutcome = /* @__PURE__ */ __name((status) => ({ kind: "externally-stopped", status }), "externalStopOutcome");
  var statusName = /* @__PURE__ */ __name((source) => source.name.trim() === "" ? "script" : source.name, "statusName");
  var statusFromStartingCancellation = /* @__PURE__ */ __name((cancellation) => ({
    ...cancellation.reason === void 0 ? {} : { reason: cancellation.reason },
    state: "stopped",
    stoppedAt: nowIso()
  }), "statusFromStartingCancellation");
  var classifyScriptTermination = /* @__PURE__ */ __name((cause) => {
    let closeClient = false;
    let logout = false;
    let stopReason;
    let sawExitRequest = false;
    let sawStopSignal = false;
    for (const reason of cause.reasons) {
      if (isInterruptReason2(reason)) {
        continue;
      }
      if (isFailReason2(reason) && reason.error instanceof ScriptStopSignal) {
        sawStopSignal = true;
        stopReason ??= reason.error.reason;
        const exitRequest = getScriptExitRequest(reason.error);
        sawExitRequest ||= exitRequest !== void 0;
        closeClient ||= exitRequest?.closeClient === true;
        logout ||= exitRequest?.logout === true;
        continue;
      }
      return { kind: "failed" };
    }
    if (sawStopSignal) {
      return sawExitRequest ? {
        exitRequest: { closeClient, logout },
        kind: "script-exited",
        ...stopReason === void 0 ? {} : { reason: stopReason }
      } : {
        kind: "script-stopped",
        ...stopReason === void 0 ? {} : { reason: stopReason }
      };
    }
    return hasInterruptsOnly2(cause) ? { kind: "script-interrupted" } : { kind: "failed" };
  }, "classifyScriptTermination");
  var causeMessage = /* @__PURE__ */ __name((cause) => {
    const squashed = squash(cause);
    return squashed instanceof Error && squashed.message.length > 0 ? attributedScriptErrorMessage(squashed) : pretty(cause);
  }, "causeMessage");
  var causeDetailsText = /* @__PURE__ */ __name((cause) => {
    const squashed = squash(cause);
    if (squashed instanceof Error) {
      const details = attributedScriptErrorDetails(squashed);
      if (details !== void 0) return details;
    }
    const pretty2 = pretty(cause);
    return pretty2.trim() === "" ? void 0 : pretty2;
  }, "causeDetailsText");
  var logScriptFailureCause = /* @__PURE__ */ __name((cause) => sync2(() => {
    const squashed = squash(cause);
    console.error(
      "[script]",
      "error running script:",
      squashed instanceof Error ? squashed : causeMessage(cause)
    );
  }), "logScriptFailureCause");
  var layer7 = effect(
    ScriptRunner,
    gen2(function* () {
      const scope3 = yield* scope2;
      const runFork3 = runForkWith2(yield* context2());
      const api2 = yield* Api;
      const bridge = yield* Bridge;
      const dialogs = yield* ScriptDialogs;
      const {
        army,
        environment,
        autoRelogin,
        autoZone,
        fileSystem: fileSystemBridge,
        http: makeHttpApi
      } = yield* ScriptServices;
      const { auth, combat, house, map: map8, packet, player, wait } = api2;
      const services = makeScriptRuntimeServices(api2, army, environment);
      const activeRef = yield* make16(null);
      const lifecycleGate = yield* make15(1);
      const latestCommandIdRef = yield* make16(0);
      const nextIdRef = yield* make16(0);
      const pendingFinalizationRef = yield* make16(
        null
      );
      const startingRef = yield* make16(null);
      const scriptOptions = makeScriptOptionsApi(yield* HostState);
      const statusRef = yield* make17({
        state: "idle"
      });
      const getStatus = /* @__PURE__ */ __name(() => get4(statusRef).pipe(map5(snapshotStatus)), "getStatus");
      const setStatus = /* @__PURE__ */ __name((status) => set3(statusRef, snapshotStatus(status)), "setStatus");
      const observe = /* @__PURE__ */ __name((changes2, snapshot, listener) => changes2.pipe(
        runForEach2(
          (value) => sync2(() => listener(snapshot(value)))
        ),
        forkIn2(scope3),
        map5((fiber2) => () => {
          runFork3(interrupt4(fiber2));
        })
      ), "observe");
      const awaitStatus = /* @__PURE__ */ __name((done4) => _await(done4).pipe(map5(snapshotStatus)), "awaitStatus");
      const requestInterrupt = fn2("ScriptRunner.requestInterrupt")(
        function* (fiber2) {
          const interruptor = yield* fiberId2;
          yield* sync2(() => {
            fiber2.interruptUnsafe(interruptor);
          });
        }
      );
      const completeFinalization = fn2("ScriptRunner.completeFinalization")(
        function* (id, done4, terminal, finalization) {
          yield* lifecycleGate.withPermit(
            gen2(function* () {
              const pending2 = yield* get3(pendingFinalizationRef);
              if (pending2?.id !== id || pending2.done !== done4) {
                yield* succeed5(done4, yield* getStatus());
                return;
              }
              yield* set2(pendingFinalizationRef, null);
              yield* setStatus(finalization.status);
              yield* succeed5(done4, snapshotStatus(finalization.status));
              if (finalization.outcome !== void 0) {
                yield* succeed5(terminal, finalization.outcome);
              }
            })
          );
        }
      );
      const finalize2 = fn2("ScriptRunner.finalize")(function* (options) {
        if (options.awaitFiber) {
          yield* await_(options.active.fiber);
        }
        yield* options.active.scope.close;
        if (options.cause !== void 0) {
          yield* logScriptFailureCause(options.cause);
        }
        if (options.beforeComplete !== void 0) {
          yield* options.beforeComplete;
        }
        const result2 = options.result();
        yield* completeFinalization(
          options.active.id,
          options.done,
          options.active.terminal,
          result2
        );
      });
      const beginFinalization = fn2("ScriptRunner.beginFinalization")(
        function* (active, options) {
          return yield* uninterruptible2(
            gen2(function* () {
              const done4 = yield* make5();
              yield* set2(activeRef, null);
              yield* set2(pendingFinalizationRef, { done: done4, id: active.id });
              if (options.intermediateStatus !== void 0) {
                yield* setStatus(options.intermediateStatus);
              }
              yield* active.scope.cancel;
              if (options.interrupt) {
                yield* requestInterrupt(active.fiber);
              }
              yield* finalize2({
                active,
                awaitFiber: options.awaitFiber,
                ...options.beforeComplete === void 0 ? {} : { beforeComplete: options.beforeComplete },
                ...options.cause === void 0 ? {} : { cause: options.cause },
                done: done4,
                result: options.result
              }).pipe(forkDetach2);
              return done4;
            })
          );
        }
      );
      const stopActive = fn2("ScriptRunner.stopActive")(function* (reason) {
        yield* update(latestCommandIdRef, (value) => value + 1);
        const result2 = yield* lifecycleGate.withPermit(
          gen2(function* () {
            const starting = yield* get3(startingRef);
            if (starting !== null) {
              yield* succeed5(
                starting.cancel,
                reason === void 0 ? {} : { reason }
              );
              return {
                done: starting.done,
                kind: "waiting"
              };
            }
            const pending2 = yield* get3(pendingFinalizationRef);
            if (pending2 !== null) {
              return {
                done: pending2.done,
                kind: "waiting"
              };
            }
            const active = yield* get3(activeRef);
            if (active === null) {
              return { kind: "status", status: yield* getStatus() };
            }
            const done4 = yield* beginFinalization(active, {
              awaitFiber: true,
              intermediateStatus: {
                name: active.name,
                state: "stopping"
              },
              interrupt: true,
              result: /* @__PURE__ */ __name(() => {
                const status = {
                  ...reason === void 0 ? {} : { reason },
                  state: "stopped",
                  stoppedAt: nowIso()
                };
                return terminalFinalization(externalStopOutcome(status));
              }, "result")
            });
            return { done: done4, kind: "waiting" };
          })
        );
        return result2.kind === "status" ? result2.status : yield* awaitStatus(result2.done);
      });
      const finishIfActive = fn2("ScriptRunner.finishIfActive")(function* (id, options) {
        const done4 = yield* lifecycleGate.withPermit(
          gen2(function* () {
            if ((yield* get3(pendingFinalizationRef)) !== null) {
              return null;
            }
            const active = yield* get3(activeRef);
            return active?.id === id ? yield* beginFinalization(active, {
              awaitFiber: false,
              ...options.beforeComplete === void 0 ? {} : { beforeComplete: options.beforeComplete },
              interrupt: false,
              result: options.result
            }) : null;
          })
        );
        if (done4 !== null) {
          yield* awaitStatus(done4);
        } else if (options.beforeComplete !== void 0) {
          yield* options.beforeComplete;
        }
      });
      const failActiveCause = fn2("ScriptRunner.failActiveCause")(
        function* (id, cause) {
          const detailsText = causeDetailsText(cause);
          const done4 = yield* lifecycleGate.withPermit(
            gen2(function* () {
              if ((yield* get3(pendingFinalizationRef)) !== null) {
                return null;
              }
              const active = yield* get3(activeRef);
              if (active?.id !== id) return null;
              return yield* beginFinalization(active, {
                awaitFiber: true,
                cause,
                interrupt: true,
                result: /* @__PURE__ */ __name(() => {
                  const status = {
                    name: active.name,
                    ...detailsText === void 0 ? {} : { detailsText },
                    failedAt: nowIso(),
                    message: causeMessage(cause),
                    state: "failed"
                  };
                  return terminalFinalization({ kind: "failed", status });
                }, "result")
              });
            })
          );
          if (done4 !== null) {
            yield* awaitStatus(done4);
          }
        }
      );
      const moveToSafeMap = makeMoveToSafeDestination({
        auth,
        bridge,
        combat,
        house,
        map: map8,
        packet,
        player,
        roomPolicy: scriptOptions.get().pipe(map5((options) => snapshotRoomPolicy(options.roomPolicy))),
        wait
      });
      const runWithSafeStartStop2 = /* @__PURE__ */ __name((main) => runWithSafeStartStop(
        gen2(function* () {
          const iterator = main();
          return yield* iterator;
        }).pipe(asVoid2),
        scriptOptions.get().pipe(map5((options) => options.safeStartStop)),
        moveToSafeMap
      ), "runWithSafeStartStop");
      const runScript = /* @__PURE__ */ __name((id, source, main) => runWithSafeStartStop2(main).pipe(
        matchCauseEffect2({
          onFailure: /* @__PURE__ */ __name((cause) => {
            const termination = classifyScriptTermination(cause);
            if (termination.kind !== "failed") {
              const reason = termination.kind === "script-interrupted" ? void 0 : termination.reason;
              return finishIfActive(id, {
                result: /* @__PURE__ */ __name(() => {
                  const status = {
                    ...reason === void 0 ? {} : { reason },
                    state: "stopped",
                    stoppedAt: nowIso()
                  };
                  const outcome2 = termination.kind === "script-exited" ? {
                    kind: "script-exited",
                    closeClient: termination.exitRequest.closeClient,
                    logout: termination.exitRequest.logout,
                    status
                  } : termination.kind === "script-stopped" ? { kind: "script-stopped", status } : { kind: "externally-stopped", status };
                  return terminalFinalization(outcome2);
                }, "result")
              }).pipe(uninterruptible2);
            }
            const detailsText = causeDetailsText(cause);
            return logScriptFailureCause(cause).pipe(
              andThen2(
                finishIfActive(id, {
                  result: /* @__PURE__ */ __name(() => {
                    const status = {
                      ...detailsText === void 0 ? {} : { detailsText },
                      failedAt: nowIso(),
                      message: causeMessage(cause),
                      name: statusName(source),
                      state: "failed"
                    };
                    return terminalFinalization({ kind: "failed", status });
                  }, "result")
                })
              )
            );
          }, "onFailure"),
          onSuccess: /* @__PURE__ */ __name(() => finishIfActive(id, {
            result: /* @__PURE__ */ __name(() => {
              const status = {
                completedAt: nowIso(),
                name: statusName(source),
                state: "completed"
              };
              return terminalFinalization({ kind: "completed", status });
            }, "result")
          }), "onSuccess")
        })
      ), "runScript");
      const completeStarting = fn2("ScriptRunner.completeStarting")(
        function* (starting, status) {
          return yield* lifecycleGate.withPermit(
            uninterruptible2(
              gen2(function* () {
                const current = yield* get3(startingRef);
                const authoritative = current?.id === starting.id && current.done === starting.done ? status : yield* getStatus();
                if (current?.id === starting.id && current.done === starting.done) {
                  yield* set2(startingRef, null);
                  yield* setStatus(status);
                }
                yield* succeed5(starting.done, snapshotStatus(status));
                if (status.state === "failed") {
                  yield* succeed5(starting.terminal, {
                    kind: "failed",
                    status: { ...status }
                  });
                } else if (status.state === "stopped") {
                  yield* succeed5(
                    starting.terminal,
                    externalStopOutcome({ ...status })
                  );
                } else if (status.state === "completed") {
                  yield* succeed5(starting.terminal, {
                    kind: "completed",
                    status: { ...status }
                  });
                }
                return authoritative;
              })
            )
          );
        }
      );
      const runStarting = fn2("ScriptRunner.runStarting")(function* (starting, source, inputs) {
        const scriptScope = makeScriptAsyncScope();
        let inputValues = {};
        let activated = false;
        let scriptFiber;
        const cleanup = suspend2(
          () => activated ? void_3 : uninterruptible2(
            gen2(function* () {
              yield* scriptScope.cancel;
              if (scriptFiber !== void 0) {
                yield* requestInterrupt(scriptFiber);
                yield* await_(scriptFiber);
              }
              yield* scriptScope.close;
            })
          )
        );
        const setup = gen2(function* () {
          yield* scriptScope.addCleanup(
            () => army.leave().pipe(catchCause2(() => void_3))
          );
          const script = makeScriptRuntimeApi({
            dialogs,
            scriptOptions,
            getInputValues: /* @__PURE__ */ __name(() => inputValues, "getInputValues"),
            log: /* @__PURE__ */ __name((...values) => console.log("[script]", ...values), "log"),
            scope: scriptScope,
            source: { sourceName: statusName(source) }
          });
          const http = yield* makeHttpApi(scriptScope);
          const modules = makeScriptBuiltinModules({
            autoRelogin,
            autoZone,
            bridge,
            failCause: /* @__PURE__ */ __name((cause) => failActiveCause(starting.id, cause), "failCause"),
            fileSystem: makeScriptFileSystemApi(fileSystemBridge, scriptScope),
            http,
            roomPolicy: scriptOptions.get().pipe(
              map5((options) => snapshotRoomPolicy(options.roomPolicy))
            ),
            scope: scriptScope,
            script,
            services
          });
          const namespace = yield* try_2({
            try: /* @__PURE__ */ __name(() => source.load(modules), "try"),
            catch: /* @__PURE__ */ __name((cause) => new ScriptLoadError({
              detail: cause instanceof Error ? cause.message : String(cause),
              cause
            }), "catch")
          });
          const loaded = yield* readScriptEntry(namespace, source.name, inputs);
          inputValues = loaded.inputValues;
          const release = yield* make5();
          const fiber2 = yield* _await(release).pipe(
            andThen2(runScript(starting.id, source, loaded.main)),
            forkDetach2
          );
          scriptFiber = fiber2;
          const active = {
            commandId: starting.commandId,
            fiber: fiber2,
            source: { ...source },
            id: starting.id,
            inputs: { ...inputs },
            name: starting.name,
            scope: scriptScope,
            terminal: starting.terminal
          };
          const status2 = {
            name: active.name,
            startedAt: nowIso(),
            state: "running"
          };
          const committed = yield* lifecycleGate.withPermit(
            uninterruptible2(
              gen2(function* () {
                const current = yield* get3(startingRef);
                const latestCommandId = yield* get3(latestCommandIdRef);
                const cancelled = yield* isDone2(starting.cancel);
                if (current?.id !== starting.id || current.done !== starting.done || latestCommandId !== starting.commandId || cancelled) {
                  return false;
                }
                yield* sync2(() => {
                  activated = true;
                });
                yield* set2(startingRef, null);
                yield* set2(activeRef, active);
                yield* setStatus(status2);
                yield* succeed5(release, void 0);
                return true;
              })
            )
          );
          if (!committed) {
            return yield* interrupt3;
          }
          return status2;
        }).pipe(ensuring2(cleanup));
        const status = yield* raceFirst2(
          setup.pipe(
            map5((status2) => ({ kind: "started", status: status2 }))
          ),
          _await(starting.cancel).pipe(
            map5(
              (cancellation) => ({ cancellation, kind: "cancelled" })
            )
          )
        ).pipe(
          matchCauseEffect2({
            onFailure: /* @__PURE__ */ __name((cause) => gen2(function* () {
              if (hasInterruptsOnly2(cause)) {
                const cancellation = (yield* isDone2(starting.cancel)) ? yield* _await(starting.cancel) : { reason: "Start cancelled" };
                return statusFromStartingCancellation(cancellation);
              }
              yield* logScriptFailureCause(cause);
              const detailsText = causeDetailsText(cause);
              return {
                ...detailsText === void 0 ? {} : { detailsText },
                failedAt: nowIso(),
                message: causeMessage(cause),
                name: starting.name,
                state: "failed"
              };
            }), "onFailure"),
            onSuccess: /* @__PURE__ */ __name((outcome2) => outcome2.kind === "started" ? succeed7(outcome2.status) : succeed7(
              statusFromStartingCancellation(outcome2.cancellation)
            ), "onSuccess")
          })
        );
        yield* completeStarting(starting, status);
      });
      const beginStarting = fn2("ScriptRunner.beginStarting")(function* (commandId, source, inputs, identity2) {
        return yield* uninterruptible2(
          gen2(function* () {
            const id = identity2?.id ?? (yield* updateAndGet(nextIdRef, (value) => value + 1));
            const cancel = yield* make5();
            const done4 = yield* make5();
            const terminal = identity2?.terminal ?? (yield* make5());
            const starting = {
              cancel,
              commandId,
              done: done4,
              id,
              name: statusName(source),
              terminal
            };
            yield* set2(startingRef, starting);
            yield* setStatus({
              name: starting.name,
              startedAt: nowIso(),
              state: "starting"
            });
            yield* runStarting(starting, source, inputs).pipe(forkDetach2);
            return starting;
          })
        );
      });
      const start3 = /* @__PURE__ */ __name((source, inputs) => {
        const sourceSnapshot = { ...source };
        const inputSnapshot = { ...inputs };
        return gen2(function* () {
          const commandId = yield* updateAndGet(
            latestCommandIdRef,
            (value) => value + 1
          );
          const identity2 = {
            id: yield* updateAndGet(nextIdRef, (value) => value + 1),
            terminal: yield* make5()
          };
          while (true) {
            const result2 = yield* lifecycleGate.withPermit(
              gen2(function* () {
                if ((yield* get3(latestCommandIdRef)) !== commandId) {
                  const status2 = {
                    reason: "Replaced by another script",
                    state: "stopped",
                    stoppedAt: nowIso()
                  };
                  yield* succeed5(
                    identity2.terminal,
                    externalStopOutcome(status2)
                  );
                  return {
                    handle: makeRunHandle(status2, identity2.terminal),
                    kind: "superseded"
                  };
                }
                const starting = yield* get3(startingRef);
                if (starting !== null) {
                  yield* succeed5(starting.cancel, {
                    reason: "Replaced by another script"
                  });
                  return {
                    done: starting.done,
                    kind: "waiting"
                  };
                }
                const pending2 = yield* get3(pendingFinalizationRef);
                if (pending2 !== null) {
                  return {
                    done: pending2.done,
                    kind: "waiting"
                  };
                }
                const active = yield* get3(activeRef);
                if (active !== null) {
                  const done4 = yield* beginFinalization(active, {
                    awaitFiber: true,
                    intermediateStatus: {
                      name: active.name,
                      state: "stopping"
                    },
                    interrupt: true,
                    result: /* @__PURE__ */ __name(() => {
                      const status2 = {
                        reason: "Replaced by another script",
                        state: "stopped",
                        stoppedAt: nowIso()
                      };
                      return terminalFinalization(externalStopOutcome(status2));
                    }, "result")
                  });
                  return { done: done4, kind: "waiting" };
                }
                const started = yield* beginStarting(
                  commandId,
                  sourceSnapshot,
                  inputSnapshot,
                  identity2
                );
                return {
                  started,
                  kind: "starting"
                };
              })
            );
            if (result2.kind === "superseded") {
              return result2.handle;
            }
            const status = yield* awaitStatus(
              result2.kind === "starting" ? result2.started.done : result2.done
            );
            if (result2.kind === "starting") {
              return makeRunHandle(status, result2.started.terminal);
            }
          }
        });
      }, "start");
      const stop2 = /* @__PURE__ */ __name((reason) => stopActive(reason), "stop");
      yield* addFinalizer3(() => stop2("shutdown").pipe(asVoid2));
      return ScriptRunner.of({
        getStatus,
        isRunning: /* @__PURE__ */ __name(() => getStatus().pipe(
          map5(
            (status) => status.state === "running" || status.state === "starting" || status.state === "stopping"
          )
        ), "isRunning"),
        onStatus: /* @__PURE__ */ __name((listener) => observe(changes(statusRef), snapshotStatus, listener), "onStatus"),
        start: start3,
        stop: stop2
      });
    })
  );

  // packages/runtime/src/host/autoRelogin.ts
  var normalizeAutoReloginDelay = /* @__PURE__ */ __name((delayMs) => Number.isFinite(delayMs) ? Math.min(3e5, Math.max(0, delayMs)) : 3e3, "normalizeAutoReloginDelay");
  var publicReloginState = /* @__PURE__ */ __name(({ config, status }) => ({
    enabled: config.enabled,
    delayMs: config.delayMs,
    captured: status.captured,
    ...status.username == null ? {} : { username: status.username },
    attempting: ["awaiting-player-ready", "awaiting-server-select", "backoff", "logging-in", "selecting-server"].includes(status.phase.tag),
    waitingDelay: status.phase.tag === "waiting-delay" || status.phase.tag === "waiting-kick",
    ...config.server === null ? {} : { server: config.server },
    ...status.attemptsRemaining === null ? {} : { attemptsRemaining: status.attemptsRemaining },
    ...status.lastError === null ? {} : { lastError: status.lastError }
  }), "publicReloginState");
  var makeAutoReloginService = /* @__PURE__ */ __name((host4) => {
    const getState = /* @__PURE__ */ __name(() => sync2(() => publicReloginState(JSON.parse(host4.read('{"kind":"autoRelogin"}')))), "getState");
    const set4 = /* @__PURE__ */ __name((payload) => sync2(() => publicReloginState(JSON.parse(host4.command(JSON.stringify(payload))))), "set");
    return {
      getState,
      getDelayMs: /* @__PURE__ */ __name(() => getState().pipe(map5((state) => state.delayMs)), "getDelayMs"),
      getServer: /* @__PURE__ */ __name(() => getState().pipe(map5((state) => state.server)), "getServer"),
      isEnabled: /* @__PURE__ */ __name(() => getState().pipe(map5((state) => state.enabled)), "isEnabled"),
      setEnabled: /* @__PURE__ */ __name((enabled) => set4({ kind: "setAutoReloginEnabled", enabled }), "setEnabled"),
      setDelay: /* @__PURE__ */ __name((delay3) => set4({ kind: "setAutoReloginDelay", delayMs: normalizeAutoReloginDelay(toMillis(delay3)) }), "setDelay"),
      setServer: /* @__PURE__ */ __name((server) => set4({ kind: "setAutoReloginServer", server: server ?? null }), "setServer")
    };
  }, "makeAutoReloginService");

  // packages/runtime/src/host/autoZone.ts
  var makeAutoZoneService = /* @__PURE__ */ __name((host4) => {
    const state = /* @__PURE__ */ __name((config) => ({
      enabled: config.enabled,
      map: config.map ?? void 0
    }), "state");
    const getState = /* @__PURE__ */ __name(() => sync2(() => state(
      JSON.parse(host4.read(JSON.stringify({ kind: "autoZone" })))
    )), "getState");
    const patch = /* @__PURE__ */ __name((patch2) => sync2(() => state(
      JSON.parse(host4.command(JSON.stringify({ kind: "patchAutoZone", patch: patch2 })))
    )), "patch");
    return {
      getState,
      getMap: /* @__PURE__ */ __name(() => getState().pipe(map5((state2) => state2.map)), "getMap"),
      isEnabled: /* @__PURE__ */ __name(() => getState().pipe(map5((state2) => state2.enabled)), "isEnabled"),
      setEnabled: /* @__PURE__ */ __name((enabled) => patch({ enabled }), "setEnabled"),
      setMap: /* @__PURE__ */ __name((map8) => patch({ map: map8 ?? null }), "setMap")
    };
  }, "makeAutoZoneService");

  // packages/runtime/src/host/filesystem.ts
  var isJson3 = is2(Json2);
  var makeFileSystemBridge = /* @__PURE__ */ __name((host4) => {
    const request = /* @__PURE__ */ __name((operation, path, signal, contents) => requestIo(host4, { service: "filesystem", operation, path: path?.toWellFormed(), contents }, signal).then((result2) => result2.ok || path === void 0 ? result2 : { ok: false, error: { ...result2.error, path } }), "request");
    return {
      exists: /* @__PURE__ */ __name((path, signal) => request("exists", path, signal), "exists"),
      list: /* @__PURE__ */ __name((path, signal) => request("list", path, signal), "list"),
      readText: /* @__PURE__ */ __name((path, signal) => request("read-text", path, signal), "readText"),
      readJson: /* @__PURE__ */ __name(async (path, signal) => {
        const result2 = await request("read-json", path, signal);
        if (!result2.ok || result2.value === void 0) return result2;
        try {
          const value = JSON.parse(result2.value);
          if (isJson3(value)) return { ok: true, value };
        } catch {
        }
        return { ok: false, error: { operation: "read-json", path, reason: "invalid-json" } };
      }, "readJson"),
      remove: /* @__PURE__ */ __name((path, signal) => request("remove", path, signal), "remove"),
      writeText: /* @__PURE__ */ __name((path, contents, signal) => request("write-text", path, signal, contents.toWellFormed()), "writeText"),
      writeJson: /* @__PURE__ */ __name((path, value, signal) => request("write-json", path, signal, `${JSON.stringify(value, null, 2)}
`), "writeJson")
    };
  }, "makeFileSystemBridge");

  // packages/runtime/src/host/environment.ts
  var makeEnvironmentService = /* @__PURE__ */ __name((host4, unavailable) => {
    const identifier2 = /* @__PURE__ */ __name((value) => Number.isFinite(value) ? value : 0, "identifier");
    const getState = /* @__PURE__ */ __name(() => sync2(() => JSON.parse(host4.read('{"kind":"environment"}'))), "getState");
    const set4 = /* @__PURE__ */ __name((edit) => try_2(() => JSON.parse(host4.command(JSON.stringify({ kind: "updateEnvironment", edit })))), "set");
    const setAutomation = /* @__PURE__ */ __name((capability, enabled) => set4({ kind: "setAutomationEnabled", capability, enabled }), "setAutomation");
    const setDropPolicy = /* @__PURE__ */ __name((patch) => set4({ kind: "updateDropPolicy", patch }), "setDropPolicy");
    const setQuestAutoRegister = /* @__PURE__ */ __name((options) => set4({ kind: "setQuestAutoRegister", ...options }), "setQuestAutoRegister");
    return {
      ...unavailable,
      getState,
      clear: /* @__PURE__ */ __name(() => set4({ kind: "clear" }), "clear"),
      addQuest: /* @__PURE__ */ __name((questId, rewardItemId) => set4({ kind: "addQuest", questId: identifier2(questId), ...rewardItemId === void 0 ? {} : { rewardItemId: identifier2(rewardItemId) } }), "addQuest"),
      removeQuest: /* @__PURE__ */ __name((questId) => set4({ kind: "removeQuest", questId: identifier2(questId) }), "removeQuest"),
      setQuestReward: /* @__PURE__ */ __name((questId, rewardItemId) => set4({ kind: "setQuestReward", questId: identifier2(questId), rewardItemId: identifier2(rewardItemId) }), "setQuestReward"),
      clearQuestReward: /* @__PURE__ */ __name((questId) => set4({ kind: "clearQuestReward", questId: identifier2(questId) }), "clearQuestReward"),
      clearQuests: /* @__PURE__ */ __name(() => set4({ kind: "clearQuests" }), "clearQuests"),
      setQuestAutoRegister,
      setAutoRegisterRequirements: /* @__PURE__ */ __name((requirements) => setQuestAutoRegister({ requirements }), "setAutoRegisterRequirements"),
      setAutoRegisterRewards: /* @__PURE__ */ __name((rewards) => setQuestAutoRegister({ rewards }), "setAutoRegisterRewards"),
      setBoostAutomationEnabled: /* @__PURE__ */ __name((enabled) => setAutomation("boosts", enabled), "setBoostAutomationEnabled"),
      setDropAutomationEnabled: /* @__PURE__ */ __name((enabled) => setAutomation("drops", enabled), "setDropAutomationEnabled"),
      setQuestAutomationEnabled: /* @__PURE__ */ __name((enabled) => setAutomation("quests", enabled), "setQuestAutomationEnabled"),
      addItem: /* @__PURE__ */ __name((name) => set4({ kind: "addItem", name }), "addItem"),
      removeItem: /* @__PURE__ */ __name((name) => set4({ kind: "removeItem", name }), "removeItem"),
      clearItems: /* @__PURE__ */ __name(() => set4({ kind: "clearItems" }), "clearItems"),
      setItemNotification: /* @__PURE__ */ __name((name, enabled) => set4({ kind: "setItemNotification", name, enabled }), "setItemNotification"),
      setItemRules: /* @__PURE__ */ __name((rules) => set4({ kind: "setItemRules", rules: { ...rules, buckets: [...rules.buckets] } }), "setItemRules"),
      setDropPolicy,
      setAcceptAcMemberOnlyDrops: /* @__PURE__ */ __name((enabled) => setDropPolicy({ acceptAcMemberOnlyDrops: enabled }), "setAcceptAcMemberOnlyDrops"),
      setAcceptAcNonMemberDrops: /* @__PURE__ */ __name((enabled) => setDropPolicy({ acceptAcNonMemberDrops: enabled }), "setAcceptAcNonMemberDrops"),
      setAcceptNonAcMemberOnlyDrops: /* @__PURE__ */ __name((enabled) => setDropPolicy({ acceptNonAcMemberOnlyDrops: enabled }), "setAcceptNonAcMemberOnlyDrops"),
      setAcceptNonAcNonMemberDrops: /* @__PURE__ */ __name((enabled) => setDropPolicy({ acceptNonAcNonMemberDrops: enabled }), "setAcceptNonAcNonMemberDrops"),
      setRejectUnregisteredDrops: /* @__PURE__ */ __name((enabled) => setDropPolicy({ rejectUnregisteredDrops: enabled }), "setRejectUnregisteredDrops"),
      addBoost: /* @__PURE__ */ __name((name) => set4({ kind: "addBoost", name }), "addBoost"),
      removeBoost: /* @__PURE__ */ __name((name) => set4({ kind: "removeBoost", name }), "removeBoost"),
      clearBoosts: /* @__PURE__ */ __name(() => set4({ kind: "clearBoosts" }), "clearBoosts")
    };
  }, "makeEnvironmentService");

  // packages/runtime/src/shared/http.ts
  var SCRIPT_HTTP_MAX_BYTES = 8 * 1024 * 1024;
  var HttpErrorReasonSchema = Literals([
    "request",
    "redirect",
    "timeout",
    "aborted",
    "too-large",
    "body"
  ]);
  var HttpFailureSchema = Struct({
    reason: HttpErrorReasonSchema,
    url: String4,
    detail: String4
  });
  var HttpError = class extends TaggedError3()("HttpError", {
    ...HttpFailureSchema.fields,
    cause: optionalKey2(Defect())
  }) {
    static {
      __name(this, "HttpError");
    }
    get message() {
      return this.detail;
    }
  };
  var HttpRequestPayloadSchema = Struct({
    sessionId: String4,
    requestId: Int.check(isGreaterThan2(0)),
    url: String4,
    method: String4,
    headers: Record(String4, String4),
    body: optionalKey2(Uint8Array2),
    deadline: optionalKey2(Finite),
    redirect: Literals(["follow", "manual", "error"]),
    maxRedirects: Int.check(isGreaterThanOrEqualTo2(0))
  });
  var HttpResponsePayloadSchema = Struct({
    body: Uint8Array2,
    headers: ArraySchema(Tuple([String4, String4])),
    status: Int,
    statusText: String4,
    url: String4
  });
  var HttpResultSchema = Union2([
    Struct({ ok: Literal2(true), value: HttpResponsePayloadSchema }),
    Struct({ ok: Literal2(false), error: HttpFailureSchema })
  ]);

  // packages/runtime/src/host/services.ts
  var HostServiceUnavailable = class extends Error {
    constructor(service3) {
      super(`The ${service3} service is not available yet.`);
      this.service = service3;
      this.name = "HostServiceUnavailable";
    }
    service;
    static {
      __name(this, "HostServiceUnavailable");
    }
  };
  var defect = /* @__PURE__ */ __name((service3) => () => die3(new HostServiceUnavailable(service3)), "defect");
  var failure = /* @__PURE__ */ __name((service3) => () => fail5(new HostServiceUnavailable(service3)), "failure");
  var armyFailure = /* @__PURE__ */ __name((ErrorClass) => () => {
    const cause = new HostServiceUnavailable("army");
    return fail5(new ErrorClass(cause.message, cause));
  }, "armyFailure");
  var unavailableEnvironment = /* @__PURE__ */ __name(() => {
    throw new HostServiceUnavailable("environment");
  }, "unavailableEnvironment");
  var environmentHost = typeof __host === "undefined" ? { read: unavailableEnvironment, command: unavailableEnvironment } : { read: __host.read, command: __host.command };
  var unavailableScriptServices = {
    army: {
      equipSet: armyFailure(ArmyError),
      getConfigString: defect("army"),
      getConfigValue: defect("army"),
      getPlayerNumber: defect("army"),
      getSession: defect("army"),
      isLeader: defect("army"),
      isMember: defect("army"),
      isStarted: defect("army"),
      joinMap: armyFailure(ArmyError),
      kill: armyFailure(ArmyError),
      killForItem: armyFailure(ArmyError),
      killForTempItem: armyFailure(ArmyError),
      leave: defect("army"),
      loopTaunt: armyFailure(ArmyLoopTauntError),
      runStep: armyFailure(ArmyError),
      start: armyFailure(ArmyError),
      sync: armyFailure(ArmyError),
      waitForAllInMap: armyFailure(ArmyError)
    },
    environment: makeEnvironmentService(environmentHost, {
      fetchBoosts: failure("environment"),
      syncToAll: failure("environment")
    }),
    http: /* @__PURE__ */ __name(() => succeed7({
      HttpError,
      request: /* @__PURE__ */ __name((url) => fail5(
        new HttpError({
          reason: "request",
          url: String(url),
          detail: new HostServiceUnavailable("http").message
        })
      ), "request")
    }), "http")
  };
  var unavailableScriptDialogs = {
    alert: defect("dialogs"),
    beep: defect("beep"),
    confirm: defect("dialogs"),
    getCurrent: defect("dialogs"),
    onCurrent: defect("dialogs"),
    prompt: defect("dialogs"),
    respond: defect("dialogs")
  };

  // packages/runtime/src/host/scripts.ts
  var api = make7(scriptApiLayer);
  api.runSync(void_3);
  var runtime = make7(
    layer7.pipe(
      provide2(
        mergeAll2(
          scriptApiLayer,
          succeed6(ScriptServices, {
            ...unavailableScriptServices,
            fileSystem: makeFileSystemBridge(__host),
            autoZone: makeAutoZoneService(__host),
            autoRelogin: makeAutoReloginService(__host)
          }),
          succeed6(ScriptDialogs, unavailableScriptDialogs)
        )
      )
    ),
    { memoMap: api.memoMap }
  );
  runtime.runSync(void_3);
  var start2 = /* @__PURE__ */ __name((load, name, inputsJson) => {
    hideInternals();
    return runtime.runPromise(
      gen2(function* () {
        const runner = yield* ScriptRunner;
        const handle = yield* runner.start(
          { name, load },
          JSON.parse(inputsJson)
        );
        const outcome2 = yield* handle.terminal;
        switch (outcome2.kind) {
          case "completed":
            return { kind: "completed" };
          case "failed":
            return {
              kind: "failed",
              message: outcome2.status.message,
              details: outcome2.status.detailsText ?? null
            };
          case "script-exited":
            return {
              kind: "script-exited",
              closeClient: outcome2.closeClient,
              logout: outcome2.logout
            };
          case "script-stopped":
          case "externally-stopped":
            return { kind: outcome2.kind, reason: outcome2.status.reason ?? null };
        }
      })
    );
  }, "start");
  var stop = /* @__PURE__ */ __name((reason) => runtime.runPromise(
    flatMap3(ScriptRunner, (runner) => runner.stop(reason)).pipe(
      asVoid2
    )
  ), "stop");

  // packages/runtime/src/host/index.ts
  Object.assign(globalThis, { __lucent: Object.freeze({ start: start2, stop, turn, wake }) });
})();
