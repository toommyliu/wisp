import { readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
import * as Schema from "effect/Schema";
import { describe, expect, it } from "vitest";
import cases from "../test-fixtures/app-data/cases.json";
import fileNames from "../test-fixtures/app-data/account-settings-file-names.json";
import { normalizeAccountManagerStorage, serializeAccountManagerStorage } from "./accounts";
import { normalizeAccountSettings, serializeAccountSettings } from "./accountSettings";

interface Issues {
  readonly reset?: ReadonlyArray<string>;
  readonly ignored?: Readonly<Record<string, number>>;
}

type Read =
  | { readonly issues: Issues }
  | { readonly unreadable: "malformed"; readonly line?: number; readonly column?: number }
  | { readonly unreadable: "not-an-object" };

const isDocument = Schema.is(Schema.Record(Schema.String, Schema.Unknown));
const isArray = Schema.is(Schema.Array(Schema.Unknown));

const mergeIssues = (issues: ReadonlyArray<Issues>): Issues => {
  const reset = [...new Set(issues.flatMap((issue) => issue.reset ?? []))];
  const ignored: Record<string, number> = {};
  for (const issue of issues) {
    for (const [key, count] of Object.entries(issue.ignored ?? {}))
      ignored[key] = (ignored[key] ?? 0) + count;
  }
  return {
    ...(reset.length > 0 ? { reset } : {}),
    ...(Object.values(ignored).some((count) => count > 0) ? { ignored } : {}),
  };
};

const decodeAccounts = (raw: Readonly<Record<string, unknown>>) => {
  const storage = normalizeAccountManagerStorage(raw);
  const reset: string[] = [];
  const accounts = isArray(raw["accounts"]) ? raw["accounts"] : [];
  const groups = isDocument(raw["groups"]) ? raw["groups"] : {};
  if (raw["accounts"] !== undefined && !isArray(raw["accounts"])) reset.push("accounts");
  if (
    (raw["groups"] !== undefined && !isDocument(raw["groups"])) ||
    Object.values(groups).some((members) => !isArray(members))
  )
    reset.push("groups");
  const ignored = {
    accounts: accounts.length - storage.accounts.length,
    groups: Object.keys(groups).length - Object.keys(storage.groups).length,
    groupMembers:
      Object.values(groups).reduce(
        (count: number, members) => count + (isArray(members) ? members.length : 0),
        0,
      ) - Object.values(storage.groups).reduce((count, members) => count + members.length, 0),
  };
  return {
    issues: mergeIssues([
      {
        ...(Object.values(ignored).some((count) => count > 0) ? { ignored } : {}),
        ...(reset.length > 0 ? { reset } : {}),
      },
    ]),
    saved: serializeAccountManagerStorage(storage),
  };
};

const isVersion = (value: unknown): value is number =>
  typeof value === "number" && Number.isInteger(value) && value >= 0;

const decodeAccountSettings = (document: Readonly<Record<string, unknown>>) => {
  const issues: Issues[] = [];
  const saved = Object.hasOwn(document, "version") ? document["version"] : 0;
  const newer = isVersion(saved) && saved > 1;
  if (!isVersion(saved)) issues.push({ reset: ["version"] });
  const settings = normalizeAccountSettings({ ...document, version: 1 });
  const reset: string[] = [];
  const scripts = document["scripts"];
  if (scripts !== undefined && !isDocument(scripts)) {
    reset.push("scripts");
  } else if (isDocument(scripts)) {
    for (const field of ["restartAfterReconnect", "roomPolicy", "safeStartStop"] as const) {
      if (scripts[field] !== undefined && !isDeepStrictEqual(scripts[field], settings.scripts[field]))
        reset.push(`scripts.${field}`);
    }
  }
  issues.push({ reset });
  return { issues: newer ? {} : mergeIssues(issues), saved: serializeAccountSettings(settings) };
};

const documents = { accounts: decodeAccounts, "account-settings": decodeAccountSettings };

const replay = (document: keyof typeof documents, bytes: Buffer): { read: Read; output?: Buffer } => {
  let raw: unknown;
  try {
    raw = JSON.parse(bytes.toString("utf8"));
  } catch (cause) {
    const at =
      cause instanceof SyntaxError
        ? / at position \d+ \(line (\d+) column (\d+)\)$/u.exec(cause.message)
        : null;
    return {
      read:
        at === null
          ? { unreadable: "malformed" }
          : { unreadable: "malformed", line: Number(at[1]), column: Number(at[2]) },
    };
  }
  if (!isDocument(raw)) return { read: { unreadable: "not-an-object" } };
  const { issues, saved } = documents[document](raw);
  return { read: { issues }, output: Buffer.from(`${JSON.stringify(saved, null, 2)}\n`, "utf8") };
};

const corpus = (path: string) => readFileSync(new URL(`../test-fixtures/app-data/${path}`, import.meta.url));

describe("app-data documents read and write like the release", () => {
  it.each(cases)("$name", (fixture) => {
    const document = fixture.document === "accounts" ? "accounts" : "account-settings";
    const { read, output } = replay(document, corpus(fixture.input));
    expect(read).toEqual(fixture.read);
    expect(output).toEqual(fixture.output === undefined ? undefined : corpus(fixture.output));
  });
});

const settingsFileName = (username: string): string | null => {
  const normalized = username.trim().toLowerCase();
  return normalized !== "" &&
    normalized !== "." &&
    normalized !== ".." &&
    !normalized.includes("/") &&
    !normalized.includes("\\") &&
    !normalized.includes("\0")
    ? `${normalized}.json`
    : null;
};

describe("account settings file names", () => {
  it.each(fileNames)("$name", ({ username, file }) => {
    expect(settingsFileName(username)).toBe(file);
  });
});
