import { describe, expect, it } from "@effect/vitest";
import * as Cause from "effect/Cause";
import * as Exit from "effect/Exit";

import {
  makeScriptExitSignal,
  ScriptStopSignal,
} from "../game/scripting/ScriptRunnerErrors";
import { classifyScriptTermination, scriptOutcome } from "./outcome";

describe("script outcome", () => {
  it.each([
    {
      name: "failure",
      cause: Cause.fail(new Error("boom")),
      expected: { kind: "failed" },
    },
    {
      name: "script stop",
      cause: Cause.fail(new ScriptStopSignal({ reason: "done" })),
      expected: { kind: "script-stopped", reason: "done" },
    },
    ...[false, true].map((actions) => ({
      name: actions ? "exit with actions" : "plain exit",
      cause: Cause.fail(
        makeScriptExitSignal({ closeClient: actions, logout: actions }),
      ),
      expected: {
        kind: "script-exited",
        exitRequest: { closeClient: actions, logout: actions },
        reason: "Requested by the script",
      },
    })),
    {
      name: "external interruption",
      cause: Cause.interrupt(),
      expected: { kind: "script-interrupted" },
    },
  ])("classifies $name", ({ cause, expected }) => {
    expect(classifyScriptTermination(cause)).toEqual(expected);
  });

  it.each([
    { name: "completion", exit: Exit.void, expected: { kind: "completed" } },
    {
      name: "a script stop",
      exit: Exit.fail(new ScriptStopSignal({ reason: "done" })),
      expected: { kind: "script-stopped", reason: "done" },
    },
    {
      name: "an exit",
      exit: Exit.fail(makeScriptExitSignal({ logout: true })),
      expected: { kind: "script-exited", closeClient: false, logout: true },
    },
    {
      name: "an interruption",
      exit: Exit.interrupt(),
      expected: { kind: "externally-stopped", reason: null },
    },
  ])("reports $name", ({ exit, expected }) => {
    expect(scriptOutcome(exit)).toEqual(expected);
  });
});
