import * as Predicate from "effect/Predicate";
import * as Numbers from "effect/Number";
import * as Result from "effect/Result";
import * as Native from "effect/Schema";

import {
  assertSync,
  baseDefinition,
  decode,
  declare,
  finite,
  message,
  size,
  type Backend,
  type Definition,
} from "./engine";
import { issue, SchemaUsageError } from "./issues";
import type { MessageOptions } from "@lucent/scripting-api/schema";
import { create, SchemaImpl } from "./Schema";

export const primitive = (
  backend: Backend,
  options?: MessageOptions,
): Definition => {
  message(options, "Invalid value.");
  if (options?.message === undefined) return baseDefinition(backend);
  const text = options.message;
  return baseDefinition(
    declare((input, parseOptions) => {
      const result = decode(backend, input, parseOptions);
      return Result.isSuccess(result)
        ? result
        : Result.fail(issue("invalid_type", text));
    }),
  );
};

export class StringImpl extends SchemaImpl<string> {
  min(length: number, options?: MessageOptions): StringImpl {
    size(length);
    return newString(
      this.constraint(
        (value) => value.length >= length,
        "too_small",
        message(options, `Use at least ${length} characters.`),
      ),
    );
  }
  max(length: number, options?: MessageOptions): StringImpl {
    size(length);
    return newString(
      this.constraint(
        (value) => value.length <= length,
        "too_big",
        message(options, `Use at most ${length} characters.`),
      ),
    );
  }
  length(length: number, options?: MessageOptions): StringImpl {
    size(length);
    return newString(
      this.constraint(
        (value) => value.length === length,
        "invalid_value",
        message(options, `Use exactly ${length} characters.`),
      ),
    );
  }
  regex(pattern: RegExp, options?: MessageOptions): StringImpl {
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
        message(options, `Use a value matching ${pattern}.`),
      ),
    );
  }
  startsWith(prefix: string, options?: MessageOptions): StringImpl {
    assertString(prefix);
    return newString(
      this.constraint(
        (value) => value.startsWith(prefix),
        "invalid_format",
        message(options, `Start with ${JSON.stringify(prefix)}.`),
      ),
    );
  }
  endsWith(suffix: string, options?: MessageOptions): StringImpl {
    assertString(suffix);
    return newString(
      this.constraint(
        (value) => value.endsWith(suffix),
        "invalid_format",
        message(options, `End with ${JSON.stringify(suffix)}.`),
      ),
    );
  }
  includes(part: string, options?: MessageOptions): StringImpl {
    assertString(part);
    return newString(
      this.constraint(
        (value) => value.includes(part),
        "invalid_format",
        message(options, `Include ${JSON.stringify(part)}.`),
      ),
    );
  }
  trim(): StringImpl {
    return newString(this.converted((value) => value.trim()));
  }
  toLowerCase(): StringImpl {
    return newString(this.converted((value) => value.toLowerCase()));
  }
  toUpperCase(): StringImpl {
    return newString(this.converted((value) => value.toUpperCase()));
  }
  normalize(form: "NFC" | "NFD" | "NFKC" | "NFKD" = "NFC"): StringImpl {
    if (!["NFC", "NFD", "NFKC", "NFKD"].includes(form))
      throw new SchemaUsageError("Unknown Unicode normalization form.");
    return newString(this.converted((value) => value.normalize(form)));
  }
}

export class NumberImpl extends SchemaImpl<number> {
  int(options?: MessageOptions): NumberImpl {
    return newNumber(
      this.constraint(
        Number.isSafeInteger,
        "invalid_value",
        message(options, "Use a safe whole number."),
      ),
    );
  }
  min(bound: number, options?: MessageOptions): NumberImpl {
    finite(bound);
    return newNumber(
      this.constraint(
        (value) => value >= bound,
        "too_small",
        message(options, `Use a number of ${bound} or more.`),
      ),
    );
  }
  max(bound: number, options?: MessageOptions): NumberImpl {
    finite(bound);
    return newNumber(
      this.constraint(
        (value) => value <= bound,
        "too_big",
        message(options, `Use a number of ${bound} or less.`),
      ),
    );
  }
  gt(bound: number, options?: MessageOptions): NumberImpl {
    finite(bound);
    return newNumber(
      this.constraint(
        (value) => value > bound,
        "too_small",
        message(options, `Use a number greater than ${bound}.`),
      ),
    );
  }
  lt(bound: number, options?: MessageOptions): NumberImpl {
    finite(bound);
    return newNumber(
      this.constraint(
        (value) => value < bound,
        "too_big",
        message(options, `Use a number less than ${bound}.`),
      ),
    );
  }
  multipleOf(divisor: number, options?: MessageOptions): NumberImpl {
    finite(divisor);
    if (divisor <= 0)
      throw new SchemaUsageError("A multiple must be greater than zero.");
    return newNumber(
      this.constraint(
        (value) => Numbers.remainder(value, divisor) === 0,
        "not_multiple_of",
        message(options, `Use a multiple of ${divisor}.`),
      ),
    );
  }
}

export class DateImpl extends SchemaImpl<Date> {
  min(bound: Date, options?: MessageOptions): DateImpl {
    const time = dateBound(bound);
    return newDate(
      this.constraint(
        (value) => value.getTime() >= time,
        "too_small",
        message(options, `Use a date on or after ${bound.toISOString()}.`),
      ),
    );
  }
  max(bound: Date, options?: MessageOptions): DateImpl {
    const time = dateBound(bound);
    return newDate(
      this.constraint(
        (value) => value.getTime() <= time,
        "too_big",
        message(options, `Use a date on or before ${bound.toISOString()}.`),
      ),
    );
  }
  gt(bound: Date, options?: MessageOptions): DateImpl {
    const time = dateBound(bound);
    return newDate(
      this.constraint(
        (value) => value.getTime() > time,
        "too_small",
        message(options, `Use a date after ${bound.toISOString()}.`),
      ),
    );
  }
  lt(bound: Date, options?: MessageOptions): DateImpl {
    const time = dateBound(bound);
    return newDate(
      this.constraint(
        (value) => value.getTime() < time,
        "too_big",
        message(options, `Use a date before ${bound.toISOString()}.`),
      ),
    );
  }
}

const assertString = (value: unknown): void => {
  if (typeof value !== "string")
    throw new SchemaUsageError("Expected a string constraint.");
};
const dateBound = (value: Date): number => {
  if (!(value instanceof Date) || !Number.isFinite(value.getTime()))
    throw new SchemaUsageError("Expected a valid date bound.");
  return value.getTime();
};
export const newString = (definition: Definition): StringImpl => {
  const schema = new StringImpl(definition);
  Object.freeze(schema);
  return schema;
};
export const newNumber = (definition: Definition): NumberImpl => {
  const schema = new NumberImpl(definition);
  Object.freeze(schema);
  return schema;
};
export const newDate = (definition: Definition): DateImpl => {
  const schema = new DateImpl(definition);
  Object.freeze(schema);
  return schema;
};

const validDate = Native.Date.check(
  Native.makeFilter((value) => Number.isFinite(value.getTime()), {
    expected: "a valid date",
  }),
);

export const string = (options?: MessageOptions): StringImpl =>
  newString(primitive(Native.String, options));
export const number = (options?: MessageOptions): NumberImpl =>
  newNumber(primitive(Native.Finite, options));
export const boolean = (options?: MessageOptions): SchemaImpl<boolean> =>
  create(primitive(Native.Boolean, options));
export const date = (options?: MessageOptions): DateImpl =>
  newDate(primitive(validDate, options));

export const literal = (
  value: unknown,
  options?: MessageOptions,
): SchemaImpl => {
  if (
    value !== null &&
    value !== undefined &&
    typeof value !== "string" &&
    typeof value !== "number" &&
    typeof value !== "boolean"
  ) {
    throw new SchemaUsageError(
      "A literal must be a string, number, boolean, null, or undefined.",
    );
  }
  const backend =
    value === undefined
      ? Native.Undefined
      : value === null
        ? Native.Null
        : Native.Literal(value);
  return create({
    ...primitive(backend, options),
    literals: Object.freeze([value]),
  });
};

export class EnumImpl extends SchemaImpl<string | number> {
  readonly values: readonly (string | number)[];
  constructor(values: readonly (string | number)[], options?: MessageOptions) {
    if (
      !Array.isArray(values) ||
      values.length === 0 ||
      !values.every(
        (value: unknown) =>
          typeof value === "string" || typeof value === "number",
      )
    ) {
      throw new SchemaUsageError(
        "An enum needs a nonempty array of strings or numbers.",
      );
    }
    const copy = Object.freeze([...new Set(values)]);
    super({ ...primitive(Native.Literals(copy), options), literals: copy });
    this.values = copy;
    Object.freeze(this);
  }
}

export const custom = (
  predicate: (value: unknown) => boolean,
  options?: MessageOptions,
): SchemaImpl => {
  if (!Predicate.isFunction(predicate))
    throw new SchemaUsageError("A custom schema requires a predicate.");
  const text = message(options, "The value does not match the required type.");
  return create(
    baseDefinition(
      Native.declare(
        (value): value is unknown => {
          const accepted = assertSync(predicate(value));
          if (typeof accepted !== "boolean")
            throw new SchemaUsageError(
              "A custom predicate must return a boolean.",
            );
          return accepted;
        },
        { title: text },
      ),
    ),
  );
};

const decimal = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
const decimalInput = Native.Union([Native.Finite, Native.String]);

const coercion = (
  source: Backend,
  convert: (value: unknown) => unknown,
  options: MessageOptions | undefined,
  expected: string,
): Definition => {
  const text = message(options, expected);
  return baseDefinition(
    declare((input, parseOptions) => {
      const accepted = decode(source, input, parseOptions);
      if (Result.isFailure(accepted))
        return Result.fail(issue("invalid_type", text));
      const value = convert(accepted.success);
      return value === rejected
        ? Result.fail(issue("invalid_value", text))
        : Result.succeed(value);
    }),
  );
};
const rejected = Symbol("invalid-coercion");

/** Reject locale-dependent dates and calendar rollovers before Date normalizes them. */
const parseDate = (input: unknown): Date | typeof rejected => {
  if (input instanceof Date)
    return Number.isFinite(input.getTime()) ? input : rejected;
  if (typeof input === "number") {
    const value = new Date(input);
    return Number.isFinite(value.getTime()) ? value : rejected;
  }
  if (typeof input !== "string") return rejected;
  const match =
    /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}):(\d{2})(?:\.\d{1,3})?(?:Z|([+-])(\d{2}):(\d{2})))?$/.exec(
      input,
    );
  if (match === null) return rejected;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
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
    31,
  ];
  if (
    month < 1 ||
    month > 12 ||
    day < 1 ||
    day > (monthDays[month - 1] ?? 0) ||
    Number(match[4] ?? 0) > 23 ||
    Number(match[5] ?? 0) > 59 ||
    Number(match[6] ?? 0) > 59 ||
    Number(match[8] ?? 0) > 23 ||
    Number(match[9] ?? 0) > 59
  )
    return rejected;
  const value = new Date(input);
  return Number.isFinite(value.getTime()) ? value : rejected;
};

export const coerce = Object.freeze({
  string: (options?: MessageOptions): StringImpl =>
    newString(
      coercion(
        Native.Union([Native.String, Native.Finite, Native.Boolean]),
        String,
        options,
        "Use a string, finite number, or boolean.",
      ),
    ),
  number: (options?: MessageOptions): NumberImpl =>
    newNumber(
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
        "Use a finite number or decimal numeric string.",
      ),
    ),
  boolean: (options?: MessageOptions): SchemaImpl<boolean> =>
    create(
      coercion(
        Native.Literals([true, false, 0, 1, "0", "1", "false", "true"]),
        (value) =>
          value === true || value === 1 || value === "1" || value === "true",
        options,
        "Use true, false, 0, or 1.",
      ),
    ),
  date: (options?: MessageOptions): DateImpl =>
    newDate(
      coercion(
        Native.Union([Native.Date, Native.Finite, Native.String]),
        parseDate,
        options,
        "Use a valid date, timestamp, or ISO date string.",
      ),
    ),
});
