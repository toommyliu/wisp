import { describe, expect, it } from "vitest";

import { scriptSchema as s } from "./index";
import type { Infer, Input, Schema } from "@lucent/scripting-api/schema";

describe("lucent/schema", () => {
  it("validates and converts inputs while preserving inference", () => {
    const options = s.object({
      mode: s.enum(["buy", "farm"]),
      quantity: s.coerce.number().int().min(1),
      limit: s.number().int().min(0).default(100),
      label: s.string().optional(),
    });
    const input: Input<typeof options> = { mode: "buy", quantity: "2" };
    const output: Infer<typeof options> = options.parse(input);
    const quantity: number = output.quantity;
    expect(quantity).toBe(2);
    expect(output).toEqual({ mode: "buy", quantity: 2, limit: 100 });
    const invalidInput: Input<typeof options> = {
      // @ts-expect-error Enum inference must retain its literal values.
      mode: "unknown",
      quantity: 2,
    };
    expect(options.is(invalidInput)).toBe(false);
    expect(options.safeParse({ mode: "buy", quantity: "" })).toMatchObject({
      success: false,
      issues: [{ path: ["quantity"], code: "invalid_value" }],
    });
  });

  it("distinguishes missing keys, undefined, defaults, and malformed optional values", () => {
    const shape = s.object({
      exact: s.string().optionalKey(),
      loose: s.string().optional(),
      defaulted: s
        .string()
        .transform((value) => value.length)
        .default("abc"),
      unknown: s.unknown(),
    });
    expect(shape.parse({ unknown: undefined })).toEqual({
      defaulted: 3,
      unknown: undefined,
    });
    expect(shape.is({})).toBe(false);
    expect(shape.is({ unknown: 1, exact: undefined })).toBe(false);
    expect(shape.is({ unknown: 1, loose: undefined })).toBe(true);
    expect(shape.is({ unknown: 1, loose: 42 })).toBe(false);
    expect(s.number().min(1).default(0).is(undefined)).toBe(false);
    expect(s.string().default("x").optional().parse(undefined)).toBeUndefined();
    expect(s.string().optional().default("x").parse(undefined)).toBe("x");
    expect(
      s.object({ key: s.string().optional().catch(undefined) }).parse({}),
    ).toEqual({ key: undefined });
    expect(
      s.object({ key: s.string().optionalKey().catch("fallback") }).parse({}),
    ).toEqual({ key: "fallback" });
    expect(
      s
        .object({ key: s.string().preprocess((value) => value ?? "x") })
        .parse({}),
    ).toEqual({ key: "x" });
    expect(
      s.object({ key: s.lazy(() => s.string().optional()) }).parse({}),
    ).toEqual({});
    let recursive: Schema<{ child?: unknown }>;
    recursive = s.object({ child: s.lazy(() => recursive).optional() });
    expect(recursive.parse({ child: {} })).toEqual({ child: {} });
    expect(shape.required(["defaulted"]).is({ unknown: 1 })).toBe(false);
  });

  it("preserves check and transformation order and validates pipeline output", () => {
    expect(s.string().trim().min(1).is("   ")).toBe(false);
    expect(s.string().min(1).trim().parse("   ")).toBe("");
    const names = s
      .string()
      .transform((value) => value.split(","))
      .pipe(s.array(s.string().trim().min(1)));
    expect(names.parse("first, second")).toEqual(["first", "second"]);
    expect(names.safeParse("first, ")).toMatchObject({
      success: false,
      issues: [{ path: [1] }],
    });
    expect(
      s.string().optional().pipe(s.string().default("x")).parse(undefined),
    ).toBe("x");
    expect(s.coerce.number().is("2")).toBe(true);
  });

  it("reports nested checks at the owning path and isolates reentrant parses", () => {
    const paths: unknown[] = [];
    const inner = s
      .object({ low: s.number(), high: s.number() })
      .check((value, context) => {
        paths.push(context.path);
        s.object({ nested: s.string() }).parse({ nested: "ok" });
        if (value.high < value.low)
          context.addIssue({
            path: ["high"],
            message: "Too low.",
            rule: "ordered",
          });
      });
    const result = s.object({ ranges: s.array(inner) }).safeParse(
      {
        ranges: [
          { low: 2, high: 1 },
          { low: 4, high: 3 },
        ],
      },
      { errors: "all" },
    );
    expect(paths).toEqual([
      ["ranges", 0],
      ["ranges", 1],
    ]);
    expect(result).toMatchObject({
      success: false,
      issues: [
        { path: ["ranges", 0, "high"], message: "Too low.", rule: "ordered" },
        { path: ["ranges", 1, "high"], message: "Too low.", rule: "ordered" },
      ],
    });
  });

  it("recovers once from schema failures without swallowing callback bugs", () => {
    let attempts = 0;
    const recover = s
      .number()
      .min(1)
      .catch(() => {
        attempts += 1;
        return 0;
      });
    expect(recover.is("bad")).toBe(false);
    expect(attempts).toBe(1);
    expect(
      s.object({ name: s.string().nullable().catch(null) }).parse({}),
    ).toEqual({ name: null });
    const bug = new Error("callback bug");
    const broken = s
      .string()
      .transform(() => {
        throw bug;
      })
      .catch("fallback");
    expect(() => broken.safeParse("ok")).toThrow(bug);
    expect(() =>
      s
        .string()
        .transform(async () => "bad")
        .safeParse("ok"),
    ).toThrow(s.SchemaUsageError);
    const rejected = s.string().transform((_, ctx) => {
      ctx.addIssue({ message: "Rejected." });
      return s.INVALID;
    });
    expect(rejected.safeParse("x")).toMatchObject({
      success: false,
      issues: [{ code: "custom", message: "Rejected." }],
    });
  });

  it("keeps unknown-key policies local and handles dangerous property names as data", () => {
    const strict = s
      .object({ child: s.object({ name: s.string() }).passthrough() })
      .strict();
    expect(strict.parse({ child: { name: "x", extra: 1 } })).toEqual({
      child: { name: "x", extra: 1 },
    });
    expect(strict.is({ child: { name: "x" }, extra: 1 })).toBe(false);
    const input: unknown = JSON.parse(
      '{"__proto__":{"polluted":true},"constructor":1}',
    );
    const output = s.record(s.unknown()).parse(input);
    expect(Object.getPrototypeOf(output)).toBe(Object.prototype);
    expect(Object.hasOwn(output, "__proto__")).toBe(true);
    expect(output["constructor"]).toBe(1);
    expect(
      s
        .object({ name: s.string() })
        .catchall(s.number())
        .is({ name: "x", extra: "bad" }),
    ).toBe(false);
  });

  it("retains valid packet entries when another entry is malformed", () => {
    const packet = s.object({ items: s.record(s.unknown()) });
    const reward = s.object({ sName: s.string().min(1) });
    const parsed = packet.parse({
      items: { first: { sName: "A" }, bad: null, last: { sName: "B" } },
    });
    const names = Object.values(parsed.items).flatMap((value) => {
      const result = reward.safeParse(value);
      return result.success ? [result.data.sName] : [];
    });
    expect(names).toEqual(["A", "B"]);
    expect(
      s.record(s.string().toLowerCase(), s.number()).safeParse({ A: 1, a: 2 }),
    ).toMatchObject({ success: false, issues: [{ code: "duplicate_key" }] });
  });

  it("bounds recursive traversal and distinguishes cycles from shared values", () => {
    type Node = { value: number; children: Node[] };
    let resolutions = 0;
    const node: Schema<Node> = s.lazy(() => {
      resolutions += 1;
      return s.object({ value: s.number(), children: s.array(node) });
    });
    const shared = { value: 1, children: [] };
    expect(node.parse({ value: 0, children: [shared, shared] })).toEqual({
      value: 0,
      children: [shared, shared],
    });
    expect(resolutions).toBe(1);
    const cycle: Node = { value: 0, children: [] };
    cycle.children.push(cycle);
    expect(node.safeParse(cycle)).toMatchObject({
      success: false,
      issues: [{ code: "cyclic_reference", path: ["children", 0] }],
    });
    expect(
      node.safeParse({ value: 0, children: [shared] }, { maxDepth: 2 }),
    ).toMatchObject({ success: false, issues: [{ code: "max_depth" }] });
    expect(node.is(shared)).toBe(true);
    const self: Schema<unknown> = s.lazy(() => self);
    expect(s.object({ self }).is({})).toBe(false);
    expect(self.is(1)).toBe(false);
  });

  it("uses ordered unions and discriminators without executing unrelated branches", () => {
    let executions = 0;
    const left = s.string().transform(() => "left");
    const right = s.string().transform(() => {
      executions += 1;
      return "right";
    });
    expect(s.union([left, right]).parse("x")).toBe("left");
    expect(executions).toBe(0);
    const event = s.discriminatedUnion("type", [
      s.object({ type: s.literal("number"), value: s.number() }),
      s.object({ type: s.literal("string"), value: right }),
    ]);
    expect(event.parse({ type: "number", value: 2 })).toEqual({
      type: "number",
      value: 2,
    });
    expect(executions).toBe(0);
    expect(() =>
      s.discriminatedUnion("type", [
        s.object({ type: s.literal("x") }),
        s.object({ type: s.literal("x") }),
      ]),
    ).toThrow(s.SchemaUsageError);
    const nullableTag = s.discriminatedUnion("type", [
      s.object({ type: s.literal("value").nullable() }),
    ]);
    expect(nullableTag.parse({ type: null })).toEqual({ type: null });
    expect(() =>
      s.discriminatedUnion("type", [
        s.object({ type: s.literal("x").catch("x").required() }),
      ]),
    ).toThrow(s.SchemaUsageError);
    const failure = s
      .object({ entry: s.union([s.object({ name: s.string() }), s.number()]) })
      .safeParse({ entry: { name: false } });
    expect(failure).toMatchObject({
      success: false,
      issues: [
        {
          path: ["entry"],
          branches: [[{ path: ["entry", "name"] }], [{ path: ["entry"] }]],
        },
      ],
    });
  });

  it("rejects implicit coercions and validates dates instead of accepting rollovers", () => {
    for (const input of [
      "",
      " ",
      "0x10",
      "Infinity",
      false,
      null,
      {},
      Number.NaN,
    ])
      expect(s.coerce.number().is(input)).toBe(false);
    expect(s.coerce.number().parse(" 1e2 ")).toBe(100);
    expect(s.number().multipleOf(0.1).is(0.3)).toBe(true);
    expect(s.number().multipleOf(0.1).is(0.31)).toBe(false);
    expect(s.coerce.boolean().parse("false")).toBe(false);
    expect(s.coerce.boolean().is("yes")).toBe(false);
    expect(s.coerce.date().is("2025-02-29")).toBe(false);
    expect(s.coerce.date().parse("2024-02-29").toISOString()).toBe(
      "2024-02-29T00:00:00.000Z",
    );
  });

  it("validates tuples, JSON, composition, regex state, and bounded diagnostics", () => {
    const tuple = s.tuple([s.string(), s.number().optional()]);
    expect(tuple.parse(["x"])).toEqual(["x"]);
    expect(tuple.is(["x", 1, 2])).toBe(false);
    expect(tuple.rest(s.boolean()).parse(["x", 1, true])).toEqual([
      "x",
      1,
      true,
    ]);
    expect(
      s.tuple([s.lazy(() => s.string()), s.number()]).parse(["x", 1]),
    ).toEqual(["x", 1]);
    expect(() =>
      s.tuple([s.lazy(() => s.string().optional()), s.number()]).parse([]),
    ).toThrow(s.SchemaUsageError);
    const pattern = /a/g;
    pattern.lastIndex = 1;
    const text = s.string().regex(pattern);
    expect([text.is("a"), text.is("a"), pattern.lastIndex]).toEqual([
      true,
      true,
      1,
    ]);
    const json = s.json(s.object({ amount: s.number() }));
    expect(json.parse('{"amount":2}')).toEqual({ amount: 2 });
    expect(json.safeParse("{")).toMatchObject({
      success: false,
      issues: [{ code: "invalid_json" }],
    });
    expect(s.jsonValue().is({ values: [null, true, "x", 2] })).toBe(true);
    expect(s.jsonValue().is({ value: undefined })).toBe(false);
    const selected = s
      .object({ a: s.string(), b: s.number() })
      .pick(["a"])
      .extend({ c: s.boolean() });
    expect(selected.parse({ a: "x", c: true, b: 1 })).toEqual({
      a: "x",
      c: true,
    });
    const failures = s
      .array(s.number())
      .safeParse(["a", "b", "c"], { errors: "all", maxIssues: 2 });
    expect(failures.success ? [] : failures.issues).toHaveLength(2);
    expect(() => s.string().parse(1)).toThrow(s.ValidationError);
    expect(() => s.string().safeParse("x", { maxDepth: 0 })).toThrow(
      s.SchemaUsageError,
    );
  });
});
