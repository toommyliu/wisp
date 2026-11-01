import { describe, expect, it } from "@effect/vitest";
import * as Cause from "effect/Cause";

import {
  classifyScriptTermination,
  statusFromStartingCancellation,
} from "./ScriptRunner";
import { makeScriptExitSignal, ScriptStopSignal } from "./ScriptRunnerErrors";

describe("ScriptRunner", () => {
  it("stops an ordinary cancelled start", () => {
    expect(
      statusFromStartingCancellation({ reason: "Connection lost" }),
    ).toMatchObject({
      reason: "Connection lost",
      state: "stopped",
    });
  });

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
});
