const host = typeof __host === "undefined" ? undefined : __host;
const text = (value: unknown): string => `${value}`.toWellFormed();
const requireArguments = (actual: number, required: number): void => {
  if (actual < required) throw new TypeError("Not enough arguments");
};
type Pair = [string, string];
type ParamsInit =
  string | Iterable<readonly [string, string]> | Record<string, string>;

let bindParams: (
  params: URLSearchParams,
  changed: (search: string) => void,
) => void;
let replaceParams: (params: URLSearchParams, search: string) => void;

export class URLSearchParams {
  #pairs: Pair[] = [];
  #changed?: (search: string) => void;

  constructor(init: ParamsInit = "") {
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
        host!.form(text(init).replace(/^\?/, ""), false),
      ) as Pair[];
  }
  get size(): number {
    return this.#pairs.length;
  }
  append(name: string, value: string): void {
    requireArguments(arguments.length, 2);
    this.#pairs.push([text(name), text(value)]);
    this.#notify();
  }
  delete(name: string, value?: string): void {
    requireArguments(arguments.length, 1);
    name = text(name);
    if (value !== undefined) value = text(value);
    this.#pairs = this.#pairs.filter(
      ([k, v]) => k !== name || (value !== undefined && v !== value),
    );
    this.#notify();
  }
  get(name: string): string | null {
    requireArguments(arguments.length, 1);
    return this.#pairs.find(([k]) => k === text(name))?.[1] ?? null;
  }
  getAll(name: string): string[] {
    requireArguments(arguments.length, 1);
    name = text(name);
    return this.#pairs.filter(([k]) => k === name).map(([, v]) => v);
  }
  has(name: string, value?: string): boolean {
    requireArguments(arguments.length, 1);
    name = text(name);
    if (value !== undefined) value = text(value);
    return this.#pairs.some(
      ([k, v]) => k === name && (value === undefined || v === value),
    );
  }
  set(name: string, value: string): void {
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
  sort(): void {
    this.#pairs.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    this.#notify();
  }
  *entries(): IterableIterator<Pair> {
    for (let i = 0; i < this.#pairs.length; i++)
      yield [...this.#pairs[i]!] as Pair;
  }
  *keys(): IterableIterator<string> {
    for (const [k] of this.entries()) yield k;
  }
  *values(): IterableIterator<string> {
    for (const [, v] of this.entries()) yield v;
  }
  [Symbol.iterator](): IterableIterator<Pair> {
    return this.entries();
  }
  forEach(
    callback: (value: string, key: string, parent: URLSearchParams) => void,
    thisArg?: unknown,
  ): void {
    if (typeof callback !== "function")
      throw new TypeError("Callback must be a function");
    for (const [k, v] of this) callback.call(thisArg, v, k, this);
  }
  toString(): string {
    return host!.form(JSON.stringify(this.#pairs), true);
  }
  #notify(): void {
    this.#changed?.(this.toString());
  }
  static {
    bindParams = (params, changed) => {
      params.#changed = changed;
    };
    replaceParams = (params, search) => {
      params.#pairs = new URLSearchParams(search).#pairs;
    };
  }
}

type Components = Record<
  | "href"
  | "origin"
  | "protocol"
  | "username"
  | "password"
  | "host"
  | "hostname"
  | "port"
  | "pathname"
  | "search"
  | "hash",
  string
>;
export class URL {
  #parts: Components;
  readonly #params: URLSearchParams;
  constructor(input: string, base?: string) {
    this.#parts = URL.#parse(
      text(input),
      base === undefined ? undefined : text(base),
    );
    this.#params = new URLSearchParams(this.#parts.search);
    bindParams(this.#params, (search) => {
      this.#parts = URL.#parse(this.href, undefined, "search", search);
    });
  }
  static #parse(
    input: string,
    base?: string,
    key?: string,
    value?: string,
  ): Components {
    const result = host!.url(input, base, key, value);
    if (result === undefined) throw new TypeError("Invalid URL");
    return JSON.parse(result) as Components;
  }
  #set(key: string, value: string): void {
    this.#parts = URL.#parse(this.href, undefined, key, text(value));
    replaceParams(this.#params, this.search);
  }
  get searchParams(): URLSearchParams {
    return this.#params;
  }
  get href(): string {
    return this.#parts.href;
  }
  set href(value: string) {
    this.#set("href", value);
  }
  get origin(): string {
    return this.#parts.origin;
  }
  get protocol(): string {
    return this.#parts.protocol;
  }
  set protocol(value: string) {
    this.#set("protocol", value);
  }
  get username(): string {
    return this.#parts.username;
  }
  set username(value: string) {
    this.#set("username", value);
  }
  get password(): string {
    return this.#parts.password;
  }
  set password(value: string) {
    this.#set("password", value);
  }
  get host(): string {
    return this.#parts.host;
  }
  set host(value: string) {
    this.#set("host", value);
  }
  get hostname(): string {
    return this.#parts.hostname;
  }
  set hostname(value: string) {
    this.#set("hostname", value);
  }
  get port(): string {
    return this.#parts.port;
  }
  set port(value: string) {
    this.#set("port", value);
  }
  get pathname(): string {
    return this.#parts.pathname;
  }
  set pathname(value: string) {
    this.#set("pathname", value);
  }
  get search(): string {
    return this.#parts.search;
  }
  set search(value: string) {
    this.#set("search", value);
  }
  get hash(): string {
    return this.#parts.hash;
  }
  set hash(value: string) {
    this.#set("hash", value);
  }
  toString(): string {
    return this.href;
  }
  toJSON(): string {
    return this.href;
  }
  static canParse(input: string, base?: string): boolean {
    requireArguments(arguments.length, 1);
    const value = text(input);
    const relative = base === undefined ? undefined : text(base);
    return host!.url(value, relative) !== undefined;
  }
}

export class TextDecoder {
  readonly #fatal: boolean;
  readonly #ignoreBOM: boolean;
  get encoding(): string {
    return "utf-8";
  }
  get fatal(): boolean {
    return this.#fatal;
  }
  get ignoreBOM(): boolean {
    return this.#ignoreBOM;
  }
  constructor(
    label = "utf-8",
    options: { fatal?: boolean; ignoreBOM?: boolean } = {},
  ) {
    if (
      !["utf-8", "utf8"].includes(
        text(label)
          .replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, "")
          .toLowerCase(),
      )
    )
      throw new RangeError("Unsupported encoding");
    this.#fatal = Boolean(options.fatal);
    this.#ignoreBOM = Boolean(options.ignoreBOM);
  }
  decode(
    input?: ArrayBuffer | ArrayBufferView,
    options: { stream?: boolean } = {},
  ): string {
    if (options.stream)
      throw new TypeError("Streaming decoding is unsupported");
    let bytes: Uint8Array;
    if (input === undefined) bytes = new Uint8Array();
    else if (ArrayBuffer.isView(input))
      bytes = new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
    else if (input instanceof ArrayBuffer) bytes = new Uint8Array(input);
    else throw new TypeError("Expected an ArrayBuffer or view");
    const decoded = host!.decode(bytes, this.fatal, this.ignoreBOM);
    if (decoded === undefined)
      throw new TypeError("The encoded data was not valid UTF-8");
    return decoded;
  }
}

const headerText = (value: unknown): string => {
  const result = `${value}`;
  if (/[^\x00-\xff]/.test(result)) throw new TypeError("Expected a ByteString");
  return result;
};
const headerName = (name: string): string => {
  name = headerText(name);
  if (!/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(name))
    throw new TypeError("Invalid header name");
  return name.toLowerCase();
};
const headerValue = (value: string): string => {
  value = headerText(value).replace(/^[\t\n\r ]+|[\t\n\r ]+$/g, "");
  if (/[\0\r\n]/.test(value)) throw new TypeError("Invalid header value");
  return value;
};
export class Headers {
  readonly #values = new Map<string, string[]>();
  constructor(
    init: Iterable<readonly [string, string]> | Record<string, string> = [],
  ) {
    const entries = Symbol.iterator in init ? init : Object.entries(init);
    for (const pair of entries) {
      if (typeof pair !== "object" || pair === null)
        throw new TypeError("Expected a name/value pair");
      const entry = [...pair];
      if (entry.length !== 2) throw new TypeError("Expected a name/value pair");
      this.append(entry[0]!, entry[1]!);
    }
  }
  append(name: string, value: string): void {
    requireArguments(arguments.length, 2);
    name = headerName(name);
    value = headerValue(value);
    const values = this.#values.get(name) ?? [];
    values.push(value);
    this.#values.set(name, values);
  }
  delete(name: string): void {
    requireArguments(arguments.length, 1);
    this.#values.delete(headerName(name));
  }
  get(name: string): string | null {
    requireArguments(arguments.length, 1);
    name = headerName(name);
    return (
      this.#values.get(name)?.join(name === "cookie" ? "; " : ", ") ?? null
    );
  }
  has(name: string): boolean {
    requireArguments(arguments.length, 1);
    return this.#values.has(headerName(name));
  }
  set(name: string, value: string): void {
    requireArguments(arguments.length, 2);
    name = headerName(name);
    value = headerValue(value);
    this.#values.set(name, [value]);
  }
  getSetCookie(): string[] {
    return [...(this.#values.get("set-cookie") ?? [])];
  }
  *entries(): IterableIterator<Pair> {
    let index = 0;
    while (true) {
      const pairs: Pair[] = [...this.#values.keys()]
        .sort()
        .flatMap((key) =>
          key === "set-cookie"
            ? this.#values.get(key)!.map((value) => [key, value] as Pair)
            : [
                [
                  key,
                  this.#values.get(key)!.join(key === "cookie" ? "; " : ", "),
                ] as Pair,
              ],
        );
      const pair = pairs[index++];
      if (!pair) return;
      yield pair;
    }
  }
  *keys(): IterableIterator<string> {
    for (const [k] of this) yield k;
  }
  *values(): IterableIterator<string> {
    for (const [, v] of this) yield v;
  }
  [Symbol.iterator](): IterableIterator<Pair> {
    return this.entries();
  }
  forEach(
    callback: (value: string, key: string, parent: Headers) => void,
    thisArg?: unknown,
  ): void {
    if (typeof callback !== "function")
      throw new TypeError("Callback must be a function");
    for (const [k, v] of this) callback.call(thisArg, v, k, this);
  }
}
