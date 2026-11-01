import * as Effect from "effect/Effect";
import * as Predicate from "effect/Predicate";
import * as Result from "effect/Result";
import * as Native from "effect/Schema";
import type * as NativeAST from "effect/SchemaAST";
import type * as NativeIssue from "effect/SchemaIssue";
import * as NativeParser from "effect/SchemaParser";

import { atPath, flatten, issue, SchemaUsageError } from "./issues";
import type {
  Issue,
  MessageOptions,
  Metadata,
  ParseOptions,
  Path,
} from "@lucent/scripting-api/schema";

export type Backend = Native.Codec<unknown>;
export type Decoded<Output = unknown> = Result.Result<
  Output,
  NativeIssue.Issue
>;
export { assertSync } from "./issues";
export const MISSING = Symbol("lucent/schema/missing");
export const INVALID = Symbol("lucent/schema/invalid");
const stateKey = Symbol("lucent/schema/parse-state");

export interface ParseState {
  readonly path: (string | number)[];
  readonly ancestors: Set<object>;
  readonly maxDepth: number;
  readonly maxIssues: number;
  readonly errors: "first" | "all";
  depth: number;
}
export interface EngineOptions extends NativeAST.ParseOptions {
  readonly [stateKey]: ParseState;
}
export type MissingPolicy = "reject" | "omit" | "parse";
export interface Definition {
  readonly backend: Backend;
  readonly missing: MissingPolicy | (() => MissingPolicy);
  readonly description?: string;
  readonly metadata: Metadata;
  readonly literals?:
    | readonly (string | number | boolean | null | undefined)[]
    | undefined;
}

const definitions = new WeakMap<object, Definition>();
const parsers = new WeakMap<
  Backend,
  ReturnType<typeof NativeParser.decodeUnknownResult<Backend>>
>();

export const register = (instance: object, definition: Definition): void => {
  definitions.set(instance, Object.freeze(definition));
};

export const isSchema = (value: unknown): value is object =>
  Predicate.isObject(value) && definitions.has(value);

export const definitionOf = (value: unknown): Definition => {
  const definition = Predicate.isObject(value)
    ? definitions.get(value)
    : undefined;
  if (definition === undefined)
    throw new SchemaUsageError("Expected a lucent/schema schema.");
  return definition;
};

export const missingOf = (definition: Definition): MissingPolicy =>
  typeof definition.missing === "function"
    ? definition.missing()
    : definition.missing;

export const stateOf = (options: NativeAST.ParseOptions): ParseState => {
  const state = (options as EngineOptions)[stateKey];
  if (state === undefined)
    throw new SchemaUsageError("Use the schema's parsing methods.");
  return state;
};

export const makeOptions = (options: ParseOptions = {}): EngineOptions => {
  if (!Predicate.isObject(options as unknown))
    throw new SchemaUsageError("Expected parsing options.");
  const errors = options.errors ?? "first";
  const maxIssues = options.maxIssues ?? 100;
  const maxDepth = options.maxDepth ?? 256;
  if (errors !== "first" && errors !== "all")
    throw new SchemaUsageError('errors must be "first" or "all".');
  positiveInteger(maxIssues, "maxIssues");
  positiveInteger(maxDepth, "maxDepth");
  if (options.message !== undefined && !Predicate.isFunction(options.message)) {
    throw new SchemaUsageError("message must be a function.");
  }
  return {
    errors,
    reportInput: false,
    [stateKey]: {
      path: [],
      ancestors: new Set(),
      maxDepth,
      maxIssues,
      errors,
      depth: 0,
    },
  };
};

/** Each immutable Effect schema gets one decoder, including schemas shared by fields. */
export const decode = <S extends Backend>(
  backend: S,
  input: unknown,
  options: NativeAST.ParseOptions,
): Decoded<S["Type"]> => {
  let parser = parsers.get(backend);
  if (parser === undefined) {
    parser = NativeParser.decodeUnknownResult(backend);
    parsers.set(backend, parser);
  }
  return parser(input, options) as Decoded<S["Type"]>;
};

/** Uses Effect's declaration extension for Lucent's field and traversal policies. */
export const declare = (
  run: (input: unknown, options: NativeAST.ParseOptions) => Decoded,
): Backend =>
  Native.declareConstructor<unknown>()([], () => (input, _ast, options) => {
    const result = run(input, options);
    return Result.isSuccess(result)
      ? Effect.succeed(result.success)
      : Effect.fail(result.failure);
  });

export const field = (
  definition: Definition,
  input: unknown,
  options: NativeAST.ParseOptions,
): Decoded => {
  if (input === MISSING) {
    const missing = missingOf(definition);
    if (missing === "omit") return Result.succeed(MISSING);
    if (missing === "reject")
      return Result.fail(issue("missing", "This field is required."));
  }
  return decode(definition.backend, input, options);
};

export const child = (
  definition: Definition,
  input: unknown,
  key: string | number,
  options: NativeAST.ParseOptions,
): Decoded => {
  const state = stateOf(options);
  state.path.push(key);
  try {
    const result = field(definition, input, options);
    return Result.isFailure(result)
      ? Result.fail(atPath([key], result.failure))
      : result;
  } finally {
    state.path.pop();
  }
};

export const container = (
  input: object,
  options: NativeAST.ParseOptions,
  run: () => Decoded,
): Decoded => {
  const state = stateOf(options);
  if (state.depth >= state.maxDepth)
    return Result.fail(
      issue("max_depth", "The value exceeds the maximum nesting depth."),
    );
  if (state.ancestors.has(input))
    return Result.fail(
      issue("cyclic_reference", "Cyclic values cannot be parsed."),
    );
  state.depth += 1;
  state.ancestors.add(input);
  try {
    return run();
  } finally {
    state.depth -= 1;
    state.ancestors.delete(input);
  }
};

export const canContinue = (
  issues: readonly Issue[],
  options: NativeAST.ParseOptions,
): boolean => {
  const state = stateOf(options);
  return (
    issues.length === 0 ||
    (state.errors === "all" && issues.length < state.maxIssues)
  );
};

export const collect = (
  issues: Issue[],
  cause: NativeIssue.Issue,
  options: NativeAST.ParseOptions,
): void => {
  const state = stateOf(options);
  issues.push(
    ...flatten(cause, {
      maxIssues: Math.max(1, state.maxIssues - issues.length),
    }),
  );
};

export const message = (
  options: MessageOptions | undefined,
  fallback: string,
): string => {
  if (
    options !== undefined &&
    (!Predicate.isObject(options as unknown) ||
      (options.message !== undefined && typeof options.message !== "string"))
  ) {
    throw new SchemaUsageError("Expected a string validation message.");
  }
  return options?.message ?? fallback;
};

export const size = (value: number, name = "length"): void => {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new SchemaUsageError(`${name} must be a nonnegative safe integer.`);
};
export const positiveInteger = (value: number, name: string): void => {
  size(value, name);
  if (value === 0)
    throw new SchemaUsageError(`${name} must be greater than zero.`);
};
export const finite = (value: number): void => {
  if (!Number.isFinite(value))
    throw new SchemaUsageError("Expected a finite numeric bound.");
};
export const validatePath = (path: Path): Path => {
  if (
    !Array.isArray(path) ||
    !path.every(
      (part: unknown) =>
        typeof part === "string" ||
        (typeof part === "number" && Number.isSafeInteger(part) && part >= 0),
    )
  ) {
    throw new SchemaUsageError(
      "An issue path must contain strings or nonnegative integer indices.",
    );
  }
  return Object.freeze([...path]);
};

export const baseDefinition = (backend: Backend): Definition => ({
  backend,
  missing: "reject",
  metadata: Object.freeze({}),
});

export const writeProperty = (
  target: object,
  key: string,
  value: unknown,
): void => {
  Object.defineProperty(target, key, {
    configurable: true,
    enumerable: true,
    writable: true,
    value,
  });
};
