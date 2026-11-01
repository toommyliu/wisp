import * as Predicate from "effect/Predicate";
import * as Result from "effect/Result";
import * as Native from "effect/Schema";

import {
  assertSync,
  baseDefinition,
  canContinue,
  child,
  collect,
  container,
  decode,
  declare,
  definitionOf,
  message,
  MISSING,
  size,
  writeProperty,
  isSchema,
  missingOf,
  type Definition,
} from "./engine";
import { fromIssues, issue, SchemaUsageError } from "./issues";
import { EnumImpl, string } from "./primitives";
import type { Issue, MessageOptions } from "@lucent/scripting-api/schema";
import { SchemaImpl } from "./Schema";

const plainObject = Native.declare<Record<string, unknown>>(
  (value): value is Record<string, unknown> =>
    Predicate.isObject(value) &&
    (Object.getPrototypeOf(value) === Object.prototype ||
      Object.getPrototypeOf(value) === null),
  { title: "a plain object" },
);
const arrayValue = Native.declare<unknown[]>(Array.isArray, {
  title: "an array",
});

type Fields = Readonly<Record<string, unknown>>;
type ObjectPolicy = "strip" | "strict" | "passthrough" | Definition;
interface ObjectConfig {
  readonly policy: ObjectPolicy;
  readonly options: MessageOptions | undefined;
}
const objectConfigs = new WeakMap<ObjectImpl, ObjectConfig>();

export class ObjectImpl extends SchemaImpl<Record<string, unknown>> {
  readonly shape: Fields;

  constructor(
    fields: Fields,
    policy: ObjectPolicy = "strip",
    options?: MessageOptions,
  ) {
    if (!Predicate.isObject(fields))
      throw new SchemaUsageError("An object schema needs a field map.");
    const shape = Object.freeze({ ...fields });
    const entries = Object.entries(shape).map(
      ([key, schema]) => [key, definitionOf(schema)] as const,
    );
    const knownKeys = new Set(entries.map(([key]) => key));
    const text = message(options, "Expected a plain object.");
    super(
      baseDefinition(
        declare((input, parseOptions) => {
          const accepted = decode(plainObject, input, parseOptions);
          if (Result.isFailure(accepted))
            return Result.fail(issue("invalid_type", text));
          const source = accepted.success;
          return container(source, parseOptions, () => {
            const output: Record<string, unknown> = {};
            const issues: Issue[] = [];
            for (const [key, definition] of entries) {
              const value = Object.hasOwn(source, key) ? source[key] : MISSING;
              const result = child(definition, value, key, parseOptions);
              if (Result.isFailure(result))
                collect(issues, result.failure, parseOptions);
              else if (result.success !== MISSING)
                writeProperty(output, key, result.success);
              if (!canContinue(issues, parseOptions)) break;
            }
            if (policy !== "strip" && canContinue(issues, parseOptions)) {
              for (const key of Object.keys(source)) {
                if (knownKeys.has(key)) continue;
                if (policy === "strict") {
                  issues.push({
                    code: "unrecognized_key",
                    path: [key],
                    message: "This field is not allowed.",
                  });
                } else if (policy === "passthrough") {
                  writeProperty(output, key, source[key]);
                } else {
                  const result = child(policy, source[key], key, parseOptions);
                  if (Result.isFailure(result))
                    collect(issues, result.failure, parseOptions);
                  else writeProperty(output, key, result.success);
                }
                if (!canContinue(issues, parseOptions)) break;
              }
            }
            return issues.length > 0
              ? Result.fail(fromIssues(issues))
              : Result.succeed(output);
          });
        }),
      ),
    );
    this.shape = shape;
    objectConfigs.set(this, {
      policy,
      options:
        options === undefined ? undefined : Object.freeze({ ...options }),
    });
    Object.freeze(this);
  }

  extend(fields: Fields): ObjectImpl {
    if (!Predicate.isObject(fields))
      throw new SchemaUsageError(
        "Expected fields to extend the object schema.",
      );
    return this.rebuild({ ...this.shape, ...fields });
  }
  pick(keys: readonly string[]): ObjectImpl {
    const selected = this.keys(keys);
    return this.rebuild(
      Object.fromEntries(
        Object.entries(this.shape).filter(([key]) => selected.has(key)),
      ),
    );
  }
  omit(keys: readonly string[]): ObjectImpl {
    const omitted = this.keys(keys);
    return this.rebuild(
      Object.fromEntries(
        Object.entries(this.shape).filter(([key]) => !omitted.has(key)),
      ),
    );
  }
  partial(keys: readonly string[] = Object.keys(this.shape)): ObjectImpl {
    const selected = this.keys(keys);
    return this.rebuild(
      Object.fromEntries(
        Object.entries(this.shape).map(([key, schema]) => [
          key,
          selected.has(key)
            ? new SchemaImpl(definitionOf(schema)).optional()
            : schema,
        ]),
      ),
    );
  }
  override required(
    keys: readonly string[] = Object.keys(this.shape),
  ): ObjectImpl {
    const selected = this.keys(keys);
    return this.rebuild(
      Object.fromEntries(
        Object.entries(this.shape).map(([key, schema]) => [
          key,
          selected.has(key)
            ? new SchemaImpl(definitionOf(schema)).required()
            : schema,
        ]),
      ),
    );
  }
  strip(): ObjectImpl {
    return this.rebuild(this.shape, "strip");
  }
  strict(): ObjectImpl {
    return this.rebuild(this.shape, "strict");
  }
  passthrough(): ObjectImpl {
    return this.rebuild(this.shape, "passthrough");
  }
  catchall(schema: unknown): ObjectImpl {
    return this.rebuild(this.shape, definitionOf(schema));
  }
  keyof(): EnumImpl {
    return new EnumImpl(Object.keys(this.shape));
  }

  private keys(keys: readonly string[]): Set<string> {
    if (
      !Array.isArray(keys) ||
      !keys.every(
        (key: unknown) =>
          typeof key === "string" && Object.hasOwn(this.shape, key),
      )
    ) {
      throw new SchemaUsageError("Select keys declared by this object schema.");
    }
    return new Set(keys);
  }
  private rebuild(fields: Fields, policy?: ObjectPolicy): ObjectImpl {
    const config = objectConfigs.get(this);
    if (config === undefined)
      throw new SchemaUsageError("Expected an object schema.");
    return new ObjectImpl(fields, policy ?? config.policy, config.options);
  }
}

export class ArrayImpl extends SchemaImpl<unknown[]> {
  readonly element: unknown;

  constructor(
    element: unknown,
    options?: MessageOptions,
    definition?: Definition,
  ) {
    const childDefinition = definitionOf(element);
    const text = message(options, "Expected an array.");
    super(
      definition ??
        baseDefinition(
          declare((input, parseOptions) => {
            const accepted = decode(arrayValue, input, parseOptions);
            if (Result.isFailure(accepted))
              return Result.fail(issue("invalid_type", text));
            const source = accepted.success;
            return container(source, parseOptions, () => {
              const output: unknown[] = [];
              const issues: Issue[] = [];
              for (let index = 0; index < source.length; index += 1) {
                const result = child(
                  childDefinition,
                  source[index],
                  index,
                  parseOptions,
                );
                if (Result.isFailure(result))
                  collect(issues, result.failure, parseOptions);
                else output.push(result.success);
                if (!canContinue(issues, parseOptions)) break;
              }
              return issues.length > 0
                ? Result.fail(fromIssues(issues))
                : Result.succeed(output);
            });
          }),
        ),
    );
    this.element = element;
    Object.freeze(this);
  }

  min(length: number, options?: MessageOptions): ArrayImpl {
    size(length);
    return new ArrayImpl(
      this.element,
      undefined,
      this.constraint(
        (value) => value.length >= length,
        "too_small",
        message(options, `Select at least ${length} entries.`),
      ),
    );
  }
  max(length: number, options?: MessageOptions): ArrayImpl {
    size(length);
    return new ArrayImpl(
      this.element,
      undefined,
      this.constraint(
        (value) => value.length <= length,
        "too_big",
        message(options, `Use at most ${length} entries.`),
      ),
    );
  }
  length(length: number, options?: MessageOptions): ArrayImpl {
    size(length);
    return new ArrayImpl(
      this.element,
      undefined,
      this.constraint(
        (value) => value.length === length,
        "invalid_value",
        message(options, `Use exactly ${length} entries.`),
      ),
    );
  }
  unique(
    selector?: (value: unknown) => unknown,
    options?: MessageOptions,
  ): ArrayImpl {
    if (selector !== undefined && !Predicate.isFunction(selector))
      throw new SchemaUsageError("A uniqueness selector must be a function.");
    return new ArrayImpl(
      this.element,
      undefined,
      this.constraint(
        (values) => {
          const keys = new Set<unknown>();
          for (const value of values) {
            const key =
              selector === undefined ? value : assertSync(selector(value));
            if (keys.has(key)) return false;
            keys.add(key);
          }
          return true;
        },
        "duplicate_value",
        message(options, "Each entry must be unique."),
      ),
    );
  }
}

const tupleConfigs = new WeakMap<
  TupleImpl,
  { readonly options: MessageOptions | undefined }
>();

export class TupleImpl extends SchemaImpl<unknown[]> {
  readonly items: readonly unknown[];

  constructor(
    items: readonly unknown[],
    rest?: unknown,
    options?: MessageOptions,
  ) {
    if (!Array.isArray(items))
      throw new SchemaUsageError("A tuple needs an array of schemas.");
    const definitions = items.map(definitionOf);
    const validateOrder = (): void => {
      let optional = false;
      for (const definition of definitions) {
        const missing = missingOf(definition);
        if (optional && missing === "reject")
          throw new SchemaUsageError(
            "Optional tuple elements must be trailing.",
          );
        optional ||= missing !== "reject";
      }
    };
    // Recursive factories may refer to the tuple being constructed.
    let orderValidated = definitions.every(
      (definition) => typeof definition.missing !== "function",
    );
    if (orderValidated) validateOrder();
    const tail = rest === undefined ? undefined : definitionOf(rest);
    const text = message(options, "Expected an array tuple.");
    super(
      baseDefinition(
        declare((input, parseOptions) => {
          if (!orderValidated) {
            validateOrder();
            orderValidated = true;
          }
          const accepted = decode(arrayValue, input, parseOptions);
          if (Result.isFailure(accepted))
            return Result.fail(issue("invalid_type", text));
          const source = accepted.success;
          return container(source, parseOptions, () => {
            const output: unknown[] = [];
            const issues: Issue[] = [];
            for (
              let index = 0;
              index < Math.max(source.length, definitions.length);
              index += 1
            ) {
              const definition = definitions[index] ?? tail;
              if (definition === undefined) {
                issues.push({
                  code: "unrecognized_key",
                  path: [index],
                  message: "This tuple has an extra element.",
                });
              } else {
                const value = Object.hasOwn(source, index)
                  ? source[index]
                  : MISSING;
                const result = child(definition, value, index, parseOptions);
                if (Result.isFailure(result))
                  collect(issues, result.failure, parseOptions);
                else if (result.success !== MISSING)
                  output[index] = result.success;
              }
              if (!canContinue(issues, parseOptions)) break;
            }
            return issues.length > 0
              ? Result.fail(fromIssues(issues))
              : Result.succeed(output);
          });
        }),
      ),
    );
    this.items = Object.freeze([...items]);
    tupleConfigs.set(this, { options });
    Object.freeze(this);
  }

  rest(schema: unknown): TupleImpl {
    return new TupleImpl(this.items, schema, tupleConfigs.get(this)?.options);
  }
}

export class RecordImpl extends SchemaImpl<Record<string, unknown>> {
  readonly key: unknown;
  readonly value: unknown;

  constructor(
    key: unknown,
    value: unknown,
    options?: MessageOptions,
    definition?: Definition,
  ) {
    const keyDefinition = definitionOf(key);
    const valueDefinition = definitionOf(value);
    const text = message(options, "Expected a plain object record.");
    super(
      definition ??
        baseDefinition(
          declare((input, parseOptions) => {
            const accepted = decode(plainObject, input, parseOptions);
            if (Result.isFailure(accepted))
              return Result.fail(issue("invalid_type", text));
            const source = accepted.success;
            return container(source, parseOptions, () => {
              const output: Record<string, unknown> = {};
              const issues: Issue[] = [];
              for (const key of Object.keys(source)) {
                const parsedKey = child(keyDefinition, key, key, parseOptions);
                if (Result.isFailure(parsedKey)) {
                  const keyIssues: Issue[] = [];
                  collect(keyIssues, parsedKey.failure, parseOptions);
                  issues.push(
                    ...keyIssues.map(
                      (entry): Issue => ({ ...entry, code: "invalid_key" }),
                    ),
                  );
                } else if (typeof parsedKey.success !== "string") {
                  throw new SchemaUsageError(
                    "Record keys must decode to strings.",
                  );
                } else if (Object.hasOwn(output, parsedKey.success)) {
                  issues.push({
                    code: "duplicate_key",
                    path: [key],
                    message: "This key duplicates another decoded key.",
                  });
                } else {
                  const result = child(
                    valueDefinition,
                    source[key],
                    key,
                    parseOptions,
                  );
                  if (Result.isFailure(result))
                    collect(issues, result.failure, parseOptions);
                  else writeProperty(output, parsedKey.success, result.success);
                }
                if (!canContinue(issues, parseOptions)) break;
              }
              return issues.length > 0
                ? Result.fail(fromIssues(issues))
                : Result.succeed(output);
            });
          }),
        ),
    );
    this.key = key;
    this.value = value;
    Object.freeze(this);
  }

  min(bound: number, options?: MessageOptions): RecordImpl {
    size(bound, "size");
    return new RecordImpl(
      this.key,
      this.value,
      undefined,
      this.constraint(
        (value) => Object.keys(value).length >= bound,
        "too_small",
        message(options, `Use at least ${bound} entries.`),
      ),
    );
  }
  max(bound: number, options?: MessageOptions): RecordImpl {
    size(bound, "size");
    return new RecordImpl(
      this.key,
      this.value,
      undefined,
      this.constraint(
        (value) => Object.keys(value).length <= bound,
        "too_big",
        message(options, `Use at most ${bound} entries.`),
      ),
    );
  }
  size(bound: number, options?: MessageOptions): RecordImpl {
    size(bound, "size");
    return new RecordImpl(
      this.key,
      this.value,
      undefined,
      this.constraint(
        (value) => Object.keys(value).length === bound,
        "invalid_value",
        message(options, `Use exactly ${bound} entries.`),
      ),
    );
  }
}

export const record = (
  first: unknown,
  second?: unknown,
  options?: MessageOptions,
): RecordImpl => {
  if (isSchema(second)) {
    return new RecordImpl(first, second, options);
  }
  return new RecordImpl(string(), first, second as MessageOptions | undefined);
};
