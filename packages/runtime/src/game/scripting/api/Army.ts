import type * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";

import type { ArmyApiRuntimeShape } from "../../army/Army";
import { ArmyLoopTauntError } from "../../army/ArmyErrors";
import type {
  ArmyLoopTauntAssignment,
  ArmyLoopTauntRuntimeAssignment,
  ArmyLoopTauntRuntimePlan,
} from "../../army/ArmyLoopTaunt";
import { ScriptExecutionError } from "../ScriptRunnerErrors";
import type { ScriptArmyApi } from "@lucent/scripting-api/contract";
import type { ScriptAsyncScope } from "../scriptAsyncScope";
import { normalizeScriptCallbackResult } from "./Callbacks";

const normalizeSkipWhen =
  (
    skipWhen: NonNullable<ArmyLoopTauntAssignment["skipWhen"]>,
  ): NonNullable<ArmyLoopTauntRuntimeAssignment["skipWhen"]> =>
  (context) =>
    Effect.try({
      try: () => skipWhen(context),
      catch: (cause) =>
        new ScriptExecutionError({
          cause,
          detail: "Loop Taunt skipWhen callback threw.",
        }),
    }).pipe(
      Effect.flatMap((result) =>
        typeof result === "boolean"
          ? Effect.succeed(result)
          : normalizeScriptCallbackResult(result),
      ),
      Effect.flatMap((result) =>
        typeof result === "boolean"
          ? Effect.succeed(result)
          : Effect.fail(
              new ScriptExecutionError({
                detail:
                  "Loop Taunt skipWhen must return a boolean, an Effect<boolean>, or a generator that returns a boolean.",
              }),
            ),
      ),
    );

export const makeScriptArmyApi = (
  army: ArmyApiRuntimeShape,
  scope: ScriptAsyncScope,
  failCause: (cause: Cause.Cause<unknown>) => Effect.Effect<void>,
): ScriptArmyApi => {
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
    loopTaunt: (plan: Parameters<ScriptArmyApi["loopTaunt"]>[0]) =>
      Effect.try({
        try: () =>
          plan.map((priorityGroup): ArmyLoopTauntRuntimePlan[number] => ({
            assignments: priorityGroup.assignments.map(
              (
                assignment: ArmyLoopTauntAssignment,
              ): ArmyLoopTauntRuntimeAssignment => {
                if (assignment.skipWhen === undefined) {
                  return {
                    players: assignment.players,
                    strategy: assignment.strategy,
                    target: assignment.target,
                  };
                }
                if (typeof assignment.skipWhen !== "function") {
                  throw new ArmyLoopTauntError(
                    "Loop Taunt skipWhen must be a function",
                  );
                }
                return {
                  ...assignment,
                  skipWhen: normalizeSkipWhen(assignment.skipWhen),
                };
              },
            ),
          })),
        catch: (cause) =>
          cause instanceof ArmyLoopTauntError
            ? cause
            : new ArmyLoopTauntError(
                "Failed to normalize Loop Taunt plan",
                cause,
              ),
      }).pipe(
        Effect.flatMap((normalized) => army.loopTaunt(normalized, failCause)),
        // addCleanup runs immediately after cancellation, closing the race
        // between handle acquisition and an already-closed script scope.
        Effect.tap((handle) => scope.addCleanup(handle.stop)),
      ),
    runStep: army.runStep,
    start: army.start,
    sync: army.sync,
    waitForAllInMap: army.waitForAllInMap,
  });
};
