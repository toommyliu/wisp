import * as Cause from "effect/Cause";
import * as Context from "effect/Context";
import * as Deferred from "effect/Deferred";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as Layer from "effect/Layer";
import * as Ref from "effect/Ref";
import * as Semaphore from "effect/Semaphore";
import * as Stream from "effect/Stream";
import * as SubscriptionRef from "effect/SubscriptionRef";

import type { ScriptInputValues } from "@lucent/schemas/scriptInputs";
import { Api } from "../flash/api/Api";
import { HostState } from "../flash/state/HostState";
import { makeScriptOptionsApi } from "../../host/scriptOptions";
import { Bridge } from "../flash/bridge/Bridge";
import type { ScriptMain } from "@lucent/scripting-api/contract";
import {
  makeScriptAsyncScope,
  type ScriptAsyncScope,
} from "./scriptAsyncScope";
import { readScriptEntry } from "./scriptEntry";
import {
  attributedScriptErrorDetails,
  attributedScriptErrorMessage,
} from "./scriptSourceAttribution";
import { makeScriptRuntimeApi, snapshotRoomPolicy } from "./ScriptRuntime";
import type { ScriptBuiltinModules } from "./ScriptBuiltinModules";
import { makeScriptBuiltinModules } from "./ScriptBuiltinModules";
import { makeScriptFileSystemApi } from "./ScriptFileSystem";
import { ScriptDialogs } from "./ScriptDialogs";
import { ScriptServices } from "./ScriptServices";
import { makeScriptRuntimeServices } from "./api/Services";
import {
  getScriptExitRequest,
  ScriptLoadError,
  ScriptStopSignal,
  type ScriptExitRequest,
} from "./ScriptRunnerErrors";

export interface ScriptSource {
  readonly name: string;
  readonly load: (builtins: ScriptBuiltinModules) => unknown;
}

export type ScriptRunnerStatus =
  | { readonly state: "idle" }
  | {
      readonly name: string;
      readonly startedAt: string;
      readonly state: "starting";
    }
  | {
      readonly name: string;
      readonly startedAt: string;
      readonly state: "running";
    }
  | {
      readonly completedAt: string;
      readonly name: string;
      readonly state: "completed";
    }
  | {
      readonly detailsText?: string;
      readonly failedAt: string;
      readonly message: string;
      readonly name: string;
      readonly state: "failed";
    }
  | {
      readonly reason?: string;
      readonly state: "stopped";
      readonly stoppedAt: string;
    }
  | {
      readonly name: string;
      readonly state: "stopping";
    }
  | {
      readonly disconnectedAt: string;
      readonly name: string;
      readonly state: "waiting-to-restart";
    };

export type StateDisposer = () => void;

export type ScriptRunTerminalOutcome =
  | {
      readonly kind: "completed";
      readonly status: Extract<
        ScriptRunnerStatus,
        { readonly state: "completed" }
      >;
    }
  | {
      readonly kind: "failed";
      readonly status: Extract<
        ScriptRunnerStatus,
        { readonly state: "failed" }
      >;
    }
  | {
      readonly kind: "script-stopped";
      readonly status: Extract<
        ScriptRunnerStatus,
        { readonly state: "stopped" }
      >;
    }
  | {
      readonly kind: "script-exited";
      readonly closeClient: boolean;
      readonly logout: boolean;
      readonly status: Extract<
        ScriptRunnerStatus,
        { readonly state: "stopped" }
      >;
    }
  | {
      readonly kind: "externally-stopped";
      readonly status: Extract<
        ScriptRunnerStatus,
        { readonly state: "stopped" }
      >;
    };

export interface ScriptRunHandle {
  readonly initialStatus: ScriptRunnerStatus;
  /** Completes after every resource owned by this run has been released. */
  readonly terminal: Effect.Effect<ScriptRunTerminalOutcome>;
}

interface ScriptFinalization {
  readonly outcome?: ScriptRunTerminalOutcome;
  readonly status: ScriptRunnerStatus;
}

const terminalFinalization = (
  outcome: ScriptRunTerminalOutcome,
): ScriptFinalization => ({ outcome, status: outcome.status });

export interface ScriptRunnerShape {
  readonly getStatus: () => Effect.Effect<ScriptRunnerStatus>;
  readonly isRunning: () => Effect.Effect<boolean>;
  readonly onStatus: (
    listener: (status: ScriptRunnerStatus) => void,
  ) => Effect.Effect<StateDisposer>;
  readonly start: (
    source: ScriptSource,
    inputs: ScriptInputValues,
  ) => Effect.Effect<ScriptRunHandle>;
  readonly stop: (reason?: string) => Effect.Effect<ScriptRunnerStatus>;
}

export class ScriptRunner extends Context.Service<
  ScriptRunner,
  ScriptRunnerShape
>()("lucent/game/scripting/ScriptRunner") {}

interface ActiveScript {
  readonly commandId: number;
  readonly fiber: Fiber.Fiber<void, unknown>;
  readonly source: ScriptSource;
  readonly id: number;
  readonly inputs: ScriptInputValues;
  readonly name: string;
  readonly scope: ScriptAsyncScope;
  readonly terminal: Deferred.Deferred<ScriptRunTerminalOutcome>;
}

interface PendingFinalization {
  readonly done: Deferred.Deferred<ScriptRunnerStatus>;
  readonly id: number;
}

interface StartingCancellation {
  readonly reason?: string;
}

interface StartingScript {
  readonly cancel: Deferred.Deferred<StartingCancellation>;
  readonly commandId: number;
  readonly done: Deferred.Deferred<ScriptRunnerStatus>;
  readonly id: number;
  readonly name: string;
  readonly terminal: Deferred.Deferred<ScriptRunTerminalOutcome>;
}

interface ScriptRunIdentity {
  readonly id: number;
  readonly terminal: Deferred.Deferred<ScriptRunTerminalOutcome>;
}

const nowIso = (): string => new Date().toISOString();

const snapshotStatus = (status: ScriptRunnerStatus): ScriptRunnerStatus => ({
  ...status,
});

const makeRunHandle = (
  initialStatus: ScriptRunnerStatus,
  terminal: Deferred.Deferred<ScriptRunTerminalOutcome>,
): ScriptRunHandle => ({
  initialStatus: snapshotStatus(initialStatus),
  terminal: Deferred.await(terminal),
});

const externalStopOutcome = (
  status: Extract<ScriptRunnerStatus, { readonly state: "stopped" }>,
): ScriptRunTerminalOutcome => ({ kind: "externally-stopped", status });

const statusName = (source: Pick<ScriptSource, "name">) =>
  source.name.trim() === "" ? "script" : source.name;

export const statusFromStartingCancellation = (
  cancellation: StartingCancellation,
): ScriptRunnerStatus => ({
  ...(cancellation.reason === undefined ? {} : { reason: cancellation.reason }),
  state: "stopped",
  stoppedAt: nowIso(),
});

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

const causeDetailsText = (cause: Cause.Cause<unknown>): string | undefined => {
  const squashed = Cause.squash(cause);
  if (squashed instanceof Error) {
    const details = attributedScriptErrorDetails(squashed);
    if (details !== undefined) return details;
  }

  const pretty = Cause.pretty(cause);
  return pretty.trim() === "" ? undefined : pretty;
};

const logScriptFailureCause = (
  cause: Cause.Cause<unknown>,
): Effect.Effect<void> =>
  Effect.sync(() => {
    const squashed = Cause.squash(cause);
    console.error(
      "[script]",
      "error running script:",
      squashed instanceof Error ? squashed : causeMessage(cause),
    );
  });

export const layer = Layer.effect(
  ScriptRunner,
  Effect.gen(function* () {
    const scope = yield* Effect.scope;
    const runFork = Effect.runForkWith(yield* Effect.context<never>());
    const api = yield* Api;
    const bridge = yield* Bridge;
    const dialogs = yield* ScriptDialogs;
    const {
      army,
      environment,
      autoRelogin,
      autoZone,
      fileSystem: fileSystemBridge,
      http: makeHttpApi,
    } = yield* ScriptServices;
    const services = makeScriptRuntimeServices(api, army, environment);

    const activeRef = yield* Ref.make<ActiveScript | null>(null);
    const lifecycleGate = yield* Semaphore.make(1);
    const latestCommandIdRef = yield* Ref.make(0);
    const nextIdRef = yield* Ref.make(0);
    const pendingFinalizationRef = yield* Ref.make<PendingFinalization | null>(
      null,
    );
    const startingRef = yield* Ref.make<StartingScript | null>(null);
    const scriptOptions = makeScriptOptionsApi(yield* HostState);
    const statusRef = yield* SubscriptionRef.make<ScriptRunnerStatus>({
      state: "idle",
    });

    const getStatus = () =>
      SubscriptionRef.get(statusRef).pipe(Effect.map(snapshotStatus));

    const setStatus = (status: ScriptRunnerStatus) =>
      SubscriptionRef.set(statusRef, snapshotStatus(status));

    const observe = <A>(
      changes: Stream.Stream<A>,
      snapshot: (value: A) => A,
      listener: (value: A) => void,
    ) =>
      changes.pipe(
        Stream.runForEach((value) =>
          Effect.sync(() => listener(snapshot(value))),
        ),
        Effect.forkIn(scope),
        Effect.map((fiber) => () => {
          runFork(Fiber.interrupt(fiber));
        }),
      );

    const awaitStatus = (done: Deferred.Deferred<ScriptRunnerStatus>) =>
      Deferred.await(done).pipe(Effect.map(snapshotStatus));

    const requestInterrupt = Effect.fn("ScriptRunner.requestInterrupt")(
      function* (fiber: Fiber.Fiber<void, unknown>) {
        const interruptor = yield* Effect.fiberId;
        yield* Effect.sync(() => {
          // Request cancellation synchronously; the owning finalizer awaits
          // termination before releasing script resources.
          fiber.interruptUnsafe(interruptor);
        });
      },
    );

    interface FinalizationOptions {
      readonly active: ActiveScript;
      readonly awaitFiber: boolean;
      readonly beforeComplete?: Effect.Effect<void>;
      readonly cause?: Cause.Cause<unknown>;
      readonly done: PendingFinalization["done"];
      readonly result: () => ScriptFinalization;
    }

    const completeFinalization = Effect.fn("ScriptRunner.completeFinalization")(
      function* (
        id: number,
        done: PendingFinalization["done"],
        terminal: Deferred.Deferred<ScriptRunTerminalOutcome>,
        finalization: ScriptFinalization,
      ) {
        yield* lifecycleGate.withPermit(
          Effect.gen(function* () {
            const pending = yield* Ref.get(pendingFinalizationRef);
            if (pending?.id !== id || pending.done !== done) {
              yield* Deferred.succeed(done, yield* getStatus());
              return;
            }

            yield* Ref.set(pendingFinalizationRef, null);
            yield* setStatus(finalization.status);
            yield* Deferred.succeed(done, snapshotStatus(finalization.status));
            if (finalization.outcome !== undefined) {
              yield* Deferred.succeed(terminal, finalization.outcome);
            }
          }),
        );
      },
    );

    const finalize = Effect.fn("ScriptRunner.finalize")(function* (
      options: FinalizationOptions,
    ) {
      if (options.awaitFiber) {
        yield* Fiber.await(options.active.fiber);
      }
      yield* options.active.scope.close;
      if (options.cause !== undefined) {
        yield* logScriptFailureCause(options.cause);
      }
      if (options.beforeComplete !== undefined) {
        yield* options.beforeComplete;
      }
      const result = options.result();
      yield* completeFinalization(
        options.active.id,
        options.done,
        options.active.terminal,
        result,
      );
    });

    const beginFinalization = Effect.fn("ScriptRunner.beginFinalization")(
      function* (
        active: ActiveScript,
        options: {
          readonly awaitFiber: boolean;
          readonly beforeComplete?: Effect.Effect<void>;
          readonly cause?: Cause.Cause<unknown>;
          readonly intermediateStatus?: ScriptRunnerStatus;
          readonly interrupt: boolean;
          readonly result: () => ScriptFinalization;
        },
      ) {
        return yield* Effect.uninterruptible(
          Effect.gen(function* () {
            const done = yield* Deferred.make<ScriptRunnerStatus>();
            yield* Ref.set(activeRef, null);
            yield* Ref.set(pendingFinalizationRef, { done, id: active.id });
            if (options.intermediateStatus !== undefined) {
              yield* setStatus(options.intermediateStatus);
            }

            yield* active.scope.cancel;
            if (options.interrupt) {
              yield* requestInterrupt(active.fiber);
            }

            yield* finalize({
              active,
              awaitFiber: options.awaitFiber,
              ...(options.beforeComplete === undefined
                ? {}
                : { beforeComplete: options.beforeComplete }),
              ...(options.cause === undefined ? {} : { cause: options.cause }),
              done,
              result: options.result,
            }).pipe(Effect.forkDetach);
            return done;
          }),
        );
      },
    );

    const stopActive = Effect.fn("ScriptRunner.stopActive")(function* (
      reason?: string,
    ) {
      yield* Ref.update(latestCommandIdRef, (value) => value + 1);
      const result = yield* lifecycleGate.withPermit(
        Effect.gen(function* () {
          const starting = yield* Ref.get(startingRef);
          if (starting !== null) {
            yield* Deferred.succeed(
              starting.cancel,
              reason === undefined ? {} : { reason },
            );
            return {
              done: starting.done,
              kind: "waiting",
            } as const;
          }

          const pending = yield* Ref.get(pendingFinalizationRef);
          if (pending !== null) {
            return {
              done: pending.done,
              kind: "waiting",
            } as const;
          }

          const active = yield* Ref.get(activeRef);
          if (active === null) {
            return { kind: "status", status: yield* getStatus() } as const;
          }

          const done = yield* beginFinalization(active, {
            awaitFiber: true,
            intermediateStatus: {
              name: active.name,
              state: "stopping",
            },
            interrupt: true,
            result: () => {
              const status = {
                ...(reason === undefined ? {} : { reason }),
                state: "stopped",
                stoppedAt: nowIso(),
              } as const;
              return terminalFinalization(externalStopOutcome(status));
            },
          });
          return { done, kind: "waiting" } as const;
        }),
      );

      return result.kind === "status"
        ? result.status
        : yield* awaitStatus(result.done);
    });

    const finishIfActive = Effect.fn("ScriptRunner.finishIfActive")(function* (
      id: number,
      options: {
        readonly beforeComplete?: Effect.Effect<void>;
        readonly result: () => ScriptFinalization;
      },
    ) {
      const done = yield* lifecycleGate.withPermit(
        Effect.gen(function* () {
          if ((yield* Ref.get(pendingFinalizationRef)) !== null) {
            return null;
          }
          const active = yield* Ref.get(activeRef);
          return active?.id === id
            ? yield* beginFinalization(active, {
                awaitFiber: false,
                ...(options.beforeComplete === undefined
                  ? {}
                  : { beforeComplete: options.beforeComplete }),
                interrupt: false,
                result: options.result,
              })
            : null;
        }),
      );
      if (done !== null) {
        yield* awaitStatus(done);
      } else if (options.beforeComplete !== undefined) {
        yield* options.beforeComplete;
      }
    });

    const failActiveCause = Effect.fn("ScriptRunner.failActiveCause")(
      function* (id: number, cause: Cause.Cause<unknown>) {
        const detailsText = causeDetailsText(cause);
        const done = yield* lifecycleGate.withPermit(
          Effect.gen(function* () {
            if ((yield* Ref.get(pendingFinalizationRef)) !== null) {
              return null;
            }

            const active = yield* Ref.get(activeRef);
            if (active?.id !== id) return null;
            return yield* beginFinalization(active, {
              awaitFiber: true,
              cause,
              interrupt: true,
              result: () => {
                const status = {
                  name: active.name,
                  ...(detailsText === undefined ? {} : { detailsText }),
                  failedAt: nowIso(),
                  message: causeMessage(cause),
                  state: "failed",
                } as const;
                return terminalFinalization({ kind: "failed", status });
              },
            });
          }),
        );
        if (done !== null) {
          yield* awaitStatus(done);
        }
      },
    );

    const runScript = (id: number, source: ScriptSource, main: ScriptMain) =>
      Effect.gen(function* () {
        const iterator = main() as Generator<
          Effect.Effect<any, any, never>,
          unknown,
          any
        >;
        return yield* iterator;
      }).pipe(
        Effect.matchCauseEffect({
          onFailure: (cause) => {
            const termination = classifyScriptTermination(cause);
            if (termination.kind !== "failed") {
              const reason =
                termination.kind === "script-interrupted"
                  ? undefined
                  : termination.reason;
              return finishIfActive(id, {
                result: () => {
                  const status = {
                    ...(reason === undefined ? {} : { reason }),
                    state: "stopped",
                    stoppedAt: nowIso(),
                  } as const;
                  const outcome: ScriptRunTerminalOutcome =
                    termination.kind === "script-exited"
                      ? {
                          kind: "script-exited",
                          closeClient: termination.exitRequest.closeClient,
                          logout: termination.exitRequest.logout,
                          status,
                        }
                      : termination.kind === "script-stopped"
                        ? { kind: "script-stopped", status }
                        : { kind: "externally-stopped", status };
                  return terminalFinalization(outcome);
                },
              }).pipe(Effect.uninterruptible);
            }

            const detailsText = causeDetailsText(cause);
            return logScriptFailureCause(cause).pipe(
              Effect.andThen(
                finishIfActive(id, {
                  result: () => {
                    const status = {
                      ...(detailsText === undefined ? {} : { detailsText }),
                      failedAt: nowIso(),
                      message: causeMessage(cause),
                      name: statusName(source),
                      state: "failed",
                    } as const;
                    return terminalFinalization({ kind: "failed", status });
                  },
                }),
              ),
            );
          },
          onSuccess: () =>
            finishIfActive(id, {
              result: () => {
                const status = {
                  completedAt: nowIso(),
                  name: statusName(source),
                  state: "completed",
                } as const;
                return terminalFinalization({ kind: "completed", status });
              },
            }),
        }),
      );

    const completeStarting = Effect.fn("ScriptRunner.completeStarting")(
      function* (starting: StartingScript, status: ScriptRunnerStatus) {
        return yield* lifecycleGate.withPermit(
          Effect.uninterruptible(
            Effect.gen(function* () {
              const current = yield* Ref.get(startingRef);
              const authoritative =
                current?.id === starting.id && current.done === starting.done
                  ? status
                  : yield* getStatus();

              if (
                current?.id === starting.id &&
                current.done === starting.done
              ) {
                yield* Ref.set(startingRef, null);
                yield* setStatus(status);
              }
              yield* Deferred.succeed(starting.done, snapshotStatus(status));
              if (status.state === "failed") {
                yield* Deferred.succeed(starting.terminal, {
                  kind: "failed",
                  status: { ...status },
                });
              } else if (status.state === "stopped") {
                yield* Deferred.succeed(
                  starting.terminal,
                  externalStopOutcome({ ...status }),
                );
              } else if (status.state === "completed") {
                yield* Deferred.succeed(starting.terminal, {
                  kind: "completed",
                  status: { ...status },
                });
              }
              return authoritative;
            }),
          ),
        );
      },
    );

    const runStarting = Effect.fn("ScriptRunner.runStarting")(function* (
      starting: StartingScript,
      source: ScriptSource,
      inputs: ScriptInputValues,
    ): Effect.fn.Return<void> {
      const scriptScope = makeScriptAsyncScope();
      let inputValues: ScriptInputValues = {};
      let activated = false;
      let scriptFiber: Fiber.Fiber<void, unknown> | undefined;

      const cleanup = Effect.suspend(() =>
        activated
          ? Effect.void
          : Effect.uninterruptible(
              Effect.gen(function* () {
                yield* scriptScope.cancel;
                if (scriptFiber !== undefined) {
                  yield* requestInterrupt(scriptFiber);
                  yield* Fiber.await(scriptFiber);
                }
                yield* scriptScope.close;
              }),
            ),
      );

      const setup = Effect.gen(function* () {
        yield* scriptScope.addCleanup(() =>
          army.leave().pipe(Effect.catchCause(() => Effect.void)),
        );
        const script = makeScriptRuntimeApi({
          dialogs,
          scriptOptions,
          getInputValues: () => inputValues,
          log: (...values) => console.log("[script]", ...values),
          scope: scriptScope,
          source: { sourceName: statusName(source) },
        });
        const http = yield* makeHttpApi(scriptScope);
        const modules = makeScriptBuiltinModules({
          autoRelogin,
          autoZone,
          bridge,
          failCause: (cause) => failActiveCause(starting.id, cause),
          fileSystem: makeScriptFileSystemApi(fileSystemBridge, scriptScope),
          http,
          roomPolicy: scriptOptions
            .get()
            .pipe(
              Effect.map((options) => snapshotRoomPolicy(options.roomPolicy)),
            ),
          scope: scriptScope,
          script,
          services,
        });
        const namespace = yield* Effect.try({
          try: () => source.load(modules),
          catch: (cause) =>
            new ScriptLoadError({
              detail: cause instanceof Error ? cause.message : String(cause),
              cause,
            }),
        });
        const loaded = yield* readScriptEntry(namespace, source.name, inputs);
        inputValues = loaded.inputValues;
        const release = yield* Deferred.make<void>();
        const fiber = yield* Deferred.await(release).pipe(
          Effect.andThen(runScript(starting.id, source, loaded.main)),
          Effect.forkDetach,
        );
        scriptFiber = fiber;
        const active: ActiveScript = {
          commandId: starting.commandId,
          fiber,
          source: { ...source },
          id: starting.id,
          inputs: { ...inputs },
          name: starting.name,
          scope: scriptScope,
          terminal: starting.terminal,
        };
        const status: ScriptRunnerStatus = {
          name: active.name,
          startedAt: nowIso(),
          state: "running",
        };

        const committed = yield* lifecycleGate.withPermit(
          Effect.uninterruptible(
            Effect.gen(function* () {
              const current = yield* Ref.get(startingRef);
              const latestCommandId = yield* Ref.get(latestCommandIdRef);
              const cancelled = yield* Deferred.isDone(starting.cancel);
              if (
                current?.id !== starting.id ||
                current.done !== starting.done ||
                latestCommandId !== starting.commandId ||
                cancelled
              ) {
                return false;
              }

              yield* Effect.sync(() => {
                activated = true;
              });
              yield* Ref.set(startingRef, null);
              yield* Ref.set(activeRef, active);
              yield* setStatus(status);
              yield* Deferred.succeed(release, undefined);
              return true;
            }),
          ),
        );
        if (!committed) {
          return yield* Effect.interrupt;
        }
        return status;
      }).pipe(Effect.ensuring(cleanup));

      const status = yield* Effect.raceFirst(
        setup.pipe(
          Effect.map((status) => ({ kind: "started", status }) as const),
        ),
        Deferred.await(starting.cancel).pipe(
          Effect.map(
            (cancellation) => ({ cancellation, kind: "cancelled" }) as const,
          ),
        ),
      ).pipe(
        Effect.matchCauseEffect({
          onFailure: (cause) =>
            Effect.gen(function* () {
              if (Cause.hasInterruptsOnly(cause)) {
                const cancellation = (yield* Deferred.isDone(starting.cancel))
                  ? yield* Deferred.await(starting.cancel)
                  : { reason: "Start cancelled" };
                return statusFromStartingCancellation(cancellation);
              }

              yield* logScriptFailureCause(cause);
              const detailsText = causeDetailsText(cause);
              return {
                ...(detailsText === undefined ? {} : { detailsText }),
                failedAt: nowIso(),
                message: causeMessage(cause),
                name: starting.name,
                state: "failed",
              } satisfies ScriptRunnerStatus;
            }),
          onSuccess: (outcome) =>
            outcome.kind === "started"
              ? Effect.succeed<ScriptRunnerStatus>(outcome.status)
              : Effect.succeed<ScriptRunnerStatus>(
                  statusFromStartingCancellation(outcome.cancellation),
                ),
        }),
      );

      yield* completeStarting(starting, status);
    });

    const beginStarting = Effect.fn("ScriptRunner.beginStarting")(function* (
      commandId: number,
      source: ScriptSource,
      inputs: ScriptInputValues,
      identity?: ScriptRunIdentity,
    ): Effect.fn.Return<StartingScript> {
      return yield* Effect.uninterruptible(
        Effect.gen(function* () {
          const id =
            identity?.id ??
            (yield* Ref.updateAndGet(nextIdRef, (value) => value + 1));
          const cancel = yield* Deferred.make<StartingCancellation>();
          const done = yield* Deferred.make<ScriptRunnerStatus>();
          const terminal =
            identity?.terminal ??
            (yield* Deferred.make<ScriptRunTerminalOutcome>());
          const starting: StartingScript = {
            cancel,
            commandId,
            done,
            id,
            name: statusName(source),
            terminal,
          };
          yield* Ref.set(startingRef, starting);
          yield* setStatus({
            name: starting.name,
            startedAt: nowIso(),
            state: "starting",
          });
          yield* runStarting(starting, source, inputs).pipe(Effect.forkDetach);
          return starting;
        }),
      );
    });

    const start: ScriptRunnerShape["start"] = (source, inputs) => {
      const sourceSnapshot = { ...source };
      const inputSnapshot = { ...inputs };
      return Effect.gen(function* () {
        const commandId = yield* Ref.updateAndGet(
          latestCommandIdRef,
          (value) => value + 1,
        );
        const identity: ScriptRunIdentity = {
          id: yield* Ref.updateAndGet(nextIdRef, (value) => value + 1),
          terminal: yield* Deferred.make<ScriptRunTerminalOutcome>(),
        };
        while (true) {
          const result = yield* lifecycleGate.withPermit(
            Effect.gen(function* () {
              if ((yield* Ref.get(latestCommandIdRef)) !== commandId) {
                const status: Extract<
                  ScriptRunnerStatus,
                  { readonly state: "stopped" }
                > = {
                  reason: "Replaced by another script",
                  state: "stopped",
                  stoppedAt: nowIso(),
                };
                yield* Deferred.succeed(
                  identity.terminal,
                  externalStopOutcome(status),
                );
                return {
                  handle: makeRunHandle(status, identity.terminal),
                  kind: "superseded",
                } as const;
              }
              const starting = yield* Ref.get(startingRef);
              if (starting !== null) {
                yield* Deferred.succeed(starting.cancel, {
                  reason: "Replaced by another script",
                });
                return {
                  done: starting.done,
                  kind: "waiting",
                } as const;
              }

              const pending = yield* Ref.get(pendingFinalizationRef);
              if (pending !== null) {
                return {
                  done: pending.done,
                  kind: "waiting",
                } as const;
              }

              const active = yield* Ref.get(activeRef);
              if (active !== null) {
                const done = yield* beginFinalization(active, {
                  awaitFiber: true,
                  intermediateStatus: {
                    name: active.name,
                    state: "stopping",
                  },
                  interrupt: true,
                  result: () => {
                    const status = {
                      reason: "Replaced by another script",
                      state: "stopped",
                      stoppedAt: nowIso(),
                    } as const;
                    return terminalFinalization(externalStopOutcome(status));
                  },
                });
                return { done, kind: "waiting" } as const;
              }

              const started = yield* beginStarting(
                commandId,
                sourceSnapshot,
                inputSnapshot,
                identity,
              );
              return {
                started,
                kind: "starting",
              } as const;
            }),
          );

          if (result.kind === "superseded") {
            return result.handle;
          }
          const status = yield* awaitStatus(
            result.kind === "starting" ? result.started.done : result.done,
          );
          if (result.kind === "starting") {
            return makeRunHandle(status, result.started.terminal);
          }
        }
      });
    };

    const stop: ScriptRunnerShape["stop"] = (reason) => stopActive(reason);
    yield* Effect.addFinalizer(() => stop("shutdown").pipe(Effect.asVoid));

    return ScriptRunner.of({
      getStatus,
      isRunning: () =>
        getStatus().pipe(
          Effect.map(
            (status) =>
              status.state === "running" ||
              status.state === "starting" ||
              status.state === "stopping",
          ),
        ),
      onStatus: (listener) =>
        observe(SubscriptionRef.changes(statusRef), snapshotStatus, listener),
      start,
      stop,
    });
  }),
);
