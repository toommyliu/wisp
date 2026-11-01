import * as NativeIssue from "effect/SchemaIssue";
import * as Predicate from "effect/Predicate";

import type { Issue, IssueCode, ParseOptions, Path } from "@lucent/scripting-api/schema";

export class SchemaUsageError extends Error {
  override readonly name = "SchemaUsageError";
}

export const assertSync = <T>(value: T): T => {
  if (Predicate.isPromiseLike(value)) {
    // A misdeclared async callback may already have started. Consume its rejection.
    void Promise.resolve(value).catch(() => undefined);
    throw new SchemaUsageError(
      "Schema callbacks must return synchronously, without promises.",
    );
  }
  return value;
};

export const formatPath = (path: Path): string => {
  let result = "$";
  for (const part of path) {
    result +=
      typeof part === "number"
        ? `[${part}]`
        : /^[A-Za-z_$][\w$]*$/.test(part)
          ? `.${part}`
          : `[${JSON.stringify(part)}]`;
  }
  return result;
};

export const formatIssues = (issues: readonly Issue[]): string =>
  issues
    .map((issue) => `${formatPath(issue.path)}: ${issue.message}`)
    .join("\n");

export class ValidationError extends Error {
  override readonly name = "ValidationError";
  readonly issues: readonly Issue[];

  constructor(issues: readonly Issue[]) {
    super(formatIssues(issues));
    this.issues = issues;
  }
}

// Keep Lucent diagnostics outside Effect annotations and never retain rejected inputs.
const diagnostics = new WeakMap<NativeIssue.Issue, readonly Issue[]>();
const nativeFormatter = NativeIssue.makeFormatterDefault();

export const issue = (
  code: IssueCode,
  message: string,
  path: Path = [],
): NativeIssue.Issue => fromIssues([{ code, message, path }]);

export const fromIssues = (issues: readonly Issue[]): NativeIssue.Issue => {
  const result = new NativeIssue.InvalidValue({
    message: issues[0]?.message ?? "Invalid value.",
  });
  diagnostics.set(result, issues);
  return result;
};

export const atPath = (
  path: Path,
  cause: NativeIssue.Issue,
): NativeIssue.Issue => new NativeIssue.Pointer(path, cause);

export const flatten = (
  cause: NativeIssue.Issue,
  options: ParseOptions = {},
  prefix: Path = [],
): readonly Issue[] => {
  const output: Issue[] = [];
  const limit = options.maxIssues ?? 100;
  const rebase = (entry: Issue, path: Path): Issue => ({
    ...entry,
    path: Object.freeze([...path, ...entry.path]),
    ...(entry.branches === undefined
      ? {}
      : {
          branches: Object.freeze(
            entry.branches.map((branch) =>
              Object.freeze(branch.map((child) => rebase(child, path))),
            ),
          ),
        }),
  });
  const append = (entry: Issue, path: Path): void => {
    if (output.length >= limit) return;
    const full = rebase(entry, path);
    const message = assertSync(options.message?.(full));
    if (message !== undefined && typeof message !== "string") {
      throw new SchemaUsageError(
        "A message callback must return a string or undefined.",
      );
    }
    output.push(
      Object.freeze(message === undefined ? full : { ...full, message }),
    );
  };
  const visit = (entry: NativeIssue.Issue, path: Path): void => {
    if (output.length >= limit) return;
    const own = diagnostics.get(entry);
    if (own !== undefined) {
      for (const diagnostic of own) append(diagnostic, path);
      return;
    }
    switch (entry._tag) {
      case "Pointer":
        visit(entry.issue, [
          ...path,
          ...entry.path.map((key) =>
            typeof key === "symbol" ? String(key) : key,
          ),
        ]);
        return;
      case "Composite":
        for (const child of entry.issues) visit(child, path);
        return;
      case "Filter":
      case "Encoding":
        visit(entry.issue, path);
        return;
      case "AnyOf":
        append(
          {
            code: "invalid_union",
            path: [],
            message: "Expected one of the allowed values or shapes.",
            branches: entry.issues.map((child) =>
              flatten(child, { maxIssues: limit }),
            ),
          },
          path,
        );
        return;
      case "Forbidden":
        throw new SchemaUsageError(
          "Schema callbacks must complete synchronously.",
        );
      default: {
        const code: IssueCode =
          entry._tag === "MissingKey"
            ? "missing"
            : entry._tag === "UnexpectedKey"
              ? "unrecognized_key"
              : entry._tag === "InvalidType"
                ? "invalid_type"
                : "invalid_value";
        append({ code, path: [], message: nativeFormatter(entry) }, path);
      }
    }
  };
  visit(cause, prefix);
  return Object.freeze(output);
};
