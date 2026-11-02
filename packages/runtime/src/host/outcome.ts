import type { ScriptOutcome } from "@lucent/game/protocol";
import * as Cause from "effect/Cause";
import * as Exit from "effect/Exit";

import {
  getScriptExitRequest,
  ScriptStopSignal,
  type ScriptExitRequest,
} from "../game/scripting/ScriptRunnerErrors";
import {
  attributedScriptErrorDetails,
  attributedScriptErrorMessage,
} from "../game/scripting/scriptSourceAttribution";

export type ScriptTermination =
  | { readonly kind: "failed" }
  | {
      readonly exitRequest: ScriptExitRequest;
      readonly kind: "script-exited";
      readonly reason?: string;
    }
  | { readonly kind: "script-interrupted" }
  | { readonly kind: "script-stopped"; readonly reason?: string };

export const classifyScriptTermination = (
  cause: Cause.Cause<unknown>,
): ScriptTermination => {
  let closeClient = false;
  let logout = false;
  let stopReason: string | undefined;
  let sawExitRequest = false;
  let sawStopSignal = false;

  for (const reason of cause.reasons) {
    if (Cause.isInterruptReason(reason)) {
      continue;
    }

    if (
      Cause.isFailReason(reason) &&
      reason.error instanceof ScriptStopSignal
    ) {
      sawStopSignal = true;
      stopReason ??= reason.error.reason;
      const exitRequest = getScriptExitRequest(reason.error);
      sawExitRequest ||= exitRequest !== undefined;
      closeClient ||= exitRequest?.closeClient === true;
      logout ||= exitRequest?.logout === true;
      continue;
    }

    return { kind: "failed" };
  }

  if (sawStopSignal) {
    return sawExitRequest
      ? {
          exitRequest: { closeClient, logout },
          kind: "script-exited",
          ...(stopReason === undefined ? {} : { reason: stopReason }),
        }
      : {
          kind: "script-stopped",
          ...(stopReason === undefined ? {} : { reason: stopReason }),
        };
  }
  return Cause.hasInterruptsOnly(cause)
    ? { kind: "script-interrupted" }
    : { kind: "failed" };
};

const causeMessage = (cause: Cause.Cause<unknown>): string => {
  const squashed = Cause.squash(cause);
  return squashed instanceof Error && squashed.message.length > 0
    ? attributedScriptErrorMessage(squashed)
    : Cause.pretty(cause);
};

const causeDetailsText = (cause: Cause.Cause<unknown>): string | null => {
  const squashed = Cause.squash(cause);
  if (squashed instanceof Error) {
    const details = attributedScriptErrorDetails(squashed);
    if (details !== undefined) return details;
  }

  const pretty = Cause.pretty(cause);
  return pretty.trim() === "" ? null : pretty;
};

export const failedOutcome = (cause: Cause.Cause<unknown>): ScriptOutcome => {
  const squashed = Cause.squash(cause);
  console.error(
    "[script]",
    "error running script:",
    squashed instanceof Error ? squashed : causeMessage(cause),
  );
  return {
    kind: "failed",
    message: causeMessage(cause),
    details: causeDetailsText(cause),
  };
};

export const scriptOutcome = (
  exit: Exit.Exit<unknown, unknown>,
): ScriptOutcome => {
  if (Exit.isSuccess(exit)) return { kind: "completed" };
  const termination = classifyScriptTermination(exit.cause);
  switch (termination.kind) {
    case "failed":
      return failedOutcome(exit.cause);
    case "script-exited":
      return {
        kind: "script-exited",
        closeClient: termination.exitRequest.closeClient,
        logout: termination.exitRequest.logout,
      };
    case "script-stopped":
      return { kind: "script-stopped", reason: termination.reason ?? null };
    case "script-interrupted":
      return { kind: "externally-stopped", reason: null };
  }
};
