import type {
  ScriptFile,
  ScriptFileReference,
  ScriptInputValues,
  ScriptInputsDefinition,
} from "@lucent/schemas/scriptInputs";

import type { ScriptRunTerminalOutcome } from "./ScriptRunner";
import { validateScriptInputValues } from "@lucent/schemas/scriptInputs";

export interface ScriptQueueEntry {
  readonly file: ScriptFileReference;
  readonly id: string;
  readonly inputsAvailable: boolean;
  readonly inputValues: ScriptInputValues;
}

export type ScriptQueuePhase =
  | "idle"
  | "paused"
  | "preparing"
  | "running"
  | "stopping";

export type ScriptQueueRunStatus =
  | "canceled"
  | "completed"
  | "exited"
  | "failed"
  | "paused"
  | "running"
  | "stopped";

export interface ScriptQueueRunItem {
  readonly durationMs?: number;
  readonly entryId: string;
  readonly file: ScriptFile;
  readonly inputValues: ScriptInputValues;
  readonly result?: ScriptRunTerminalOutcome;
  readonly state: "active" | "finished" | "pending";
}

export interface ScriptQueueRun {
  readonly items: readonly ScriptQueueRunItem[];
  readonly status: ScriptQueueRunStatus;
}

export interface ScriptQueueState {
  readonly attentionEntryId?: string | undefined;
  readonly currentIndex: number | null;
  readonly entries: readonly ScriptQueueEntry[];
  readonly error?: string | undefined;
  readonly latestRun: ScriptQueueRun | null;
  readonly phase: ScriptQueuePhase;
}

export type ScriptQueueInputReason = "add" | "edit" | "preflight";

export interface ScriptQueueInputRequest {
  readonly definition: ScriptInputsDefinition;
  readonly entryId?: string;
  readonly file: ScriptFile;
  readonly reason: ScriptQueueInputReason;
  readonly signal: AbortSignal;
  readonly values: ScriptInputValues;
}

export interface ScriptQueueSession {
  readonly terminal: Promise<ScriptRunTerminalOutcome>;
}

export interface ScriptQueueDependencies {
  readonly confirmStandaloneReplacement: (
    signal: AbortSignal,
  ) => Promise<boolean>;
  readonly createId: (prefix: string) => string;
  readonly isRunnerActive: () => Promise<boolean>;
  readonly now?: () => number;
  readonly onUnexpectedError: (cause: unknown) => void;
  readonly requestInputs: (
    request: ScriptQueueInputRequest,
  ) => Promise<ScriptInputValues | null>;
  readonly resolve: (reference: ScriptFileReference) => Promise<ScriptFile>;
  readonly startScript: (
    file: ScriptFile,
    inputValues: ScriptInputValues,
  ) => Promise<ScriptQueueSession>;
  readonly stopScript: (reason: string) => Promise<void>;
}

export interface ScriptQueue {
  readonly add: (
    file: ScriptFile,
    inputValues: ScriptInputValues,
  ) => Promise<string | null>;
  readonly cancel: (reason?: string) => Promise<void>;
  readonly clear: () => void;
  readonly dispose: () => void;
  readonly editInputs: (entryId: string) => Promise<boolean>;
  readonly getState: () => ScriptQueueState;
  readonly move: (entryId: string, offset: -1 | 1) => void;
  readonly onState: (listener: (state: ScriptQueueState) => void) => () => void;
  readonly remove: (entryId: string) => void;
  readonly runNext: () => void;
  readonly start: () => Promise<boolean>;
}

class ScriptQueuePreparationError extends Error {
  readonly entryId?: string | undefined;

  constructor(message: string, entryId?: string) {
    super(message);
    this.name = "ScriptQueuePreparationError";
    this.entryId = entryId;
  }
}

const errorMessage = (cause: unknown, fallback: string): string =>
  cause instanceof Error && cause.message.trim() !== ""
    ? cause.message
    : fallback;

const fileReference = (file: ScriptFile): ScriptFileReference => ({
  name: file.name,
  path: file.path,
  ...(file.reference === undefined ? {} : { reference: file.reference }),
});

const queueEntryFor = (
  id: string,
  file: ScriptFile,
  inputValues: ScriptInputValues,
): ScriptQueueEntry => ({
  file: fileReference(file),
  id,
  inputsAvailable: (file.inputs?.fields.length ?? 0) > 0,
  inputValues: { ...inputValues },
});

export const makeScriptQueue = (
  dependencies: ScriptQueueDependencies,
): ScriptQueue => {
  const now = dependencies.now ?? Date.now;
  const listeners = new Set<(state: ScriptQueueState) => void>();
  let state: ScriptQueueState = {
    currentIndex: null,
    entries: [],
    latestRun: null,
    phase: "idle",
  };
  let draftController: AbortController | null = null;
  let preparationController: AbortController | null = null;
  let runVersion = 0;
  let activeSettlement: Promise<void> | null = null;
  let disposed = false;

  const getState = (): ScriptQueueState => state;

  const emit = (next: ScriptQueueState): void => {
    state = next;
    if (disposed) return;
    for (const listener of listeners) listener(state);
  };

  const patchState = (patch: Partial<ScriptQueueState>): void => {
    emit({ ...state, ...patch });
  };

  const replaceEntry = (entry: ScriptQueueEntry): void => {
    patchState({
      entries: state.entries.map((current) =>
        current.id === entry.id ? entry : current,
      ),
    });
  };

  const requestValidatedInputs = async (
    file: ScriptFile,
    values: ScriptInputValues,
    reason: ScriptQueueInputReason,
    signal: AbortSignal,
    entryId?: string,
  ): Promise<ScriptInputValues | null> => {
    const definition = file.inputs;
    if (definition === null) return {};

    const validation = validateScriptInputValues(definition, values);
    if (reason !== "edit" && validation.status === "ok") {
      return validation.values;
    }

    const edited = await dependencies.requestInputs({
      definition,
      ...(entryId === undefined ? {} : { entryId }),
      file,
      reason,
      signal,
      values: validation.values,
    });
    if (edited === null) return null;

    const result = validateScriptInputValues(definition, edited);
    if (result.status === "missing-required") {
      throw new ScriptQueuePreparationError(
        "Required script inputs are still missing.",
        entryId,
      );
    }
    return result.values;
  };

  const add: ScriptQueue["add"] = async (file, inputValues) => {
    if (state.phase !== "idle" || draftController !== null) return null;
    const controller = new AbortController();
    draftController = controller;
    try {
      const values = await requestValidatedInputs(
        file,
        inputValues,
        "add",
        controller.signal,
      );
      if (values === null || draftController !== controller) {
        return null;
      }

      const id = dependencies.createId("queue");
      const entry = queueEntryFor(id, file, values);
      patchState({
        attentionEntryId: undefined,
        entries: [...state.entries, entry],
        error: undefined,
      });
      return id;
    } finally {
      if (draftController === controller) draftController = null;
    }
  };

  const editInputs: ScriptQueue["editInputs"] = async (entryId) => {
    if (state.phase !== "idle" || draftController !== null) return false;
    const entry = state.entries.find((candidate) => candidate.id === entryId);
    if (entry === undefined) return false;

    const controller = new AbortController();
    draftController = controller;
    try {
      const file = await dependencies.resolve(entry.file);
      if (draftController !== controller) {
        return false;
      }
      const values = await requestValidatedInputs(
        file,
        entry.inputValues,
        "edit",
        controller.signal,
        entry.id,
      );
      if (values === null || draftController !== controller) {
        return false;
      }

      replaceEntry(queueEntryFor(entry.id, file, values));
      patchState({ attentionEntryId: undefined, error: undefined });
      return true;
    } catch (cause) {
      if (draftController === controller) {
        patchState({
          attentionEntryId: entry.id,
          error: errorMessage(cause, `Failed to configure ${entry.file.name}.`),
        });
      }
      return false;
    } finally {
      if (draftController === controller) draftController = null;
    }
  };

  const finishRun = (
    version: number,
    status: Exclude<ScriptQueueRunStatus, "paused" | "running">,
  ): void => {
    if (version !== runVersion || state.latestRun === null) return;
    patchState({
      currentIndex: null,
      latestRun: {
        ...state.latestRun,
        status,
      },
      phase: "idle",
    });
  };

  const applyTerminalOutcome = (
    version: number,
    index: number,
    outcome: ScriptRunTerminalOutcome,
    startedAtMs: number,
  ): ScriptQueueRun | null => {
    const run = state.latestRun;
    if (version !== runVersion || run === null) return null;
    const items = run.items.map((item, itemIndex) =>
      itemIndex === index
        ? {
            ...item,
            durationMs: Math.max(0, now() - startedAtMs),
            result: outcome,
            state: "finished" as const,
          }
        : item,
    );
    const latestRun = { ...run, items };
    patchState({ latestRun });
    return latestRun;
  };

  const unexpectedFailure = (
    item: ScriptQueueRunItem,
    cause: unknown,
  ): ScriptRunTerminalOutcome => ({
    kind: "failed",
    status: {
      failedAt: new Date(now()).toISOString(),
      message: errorMessage(cause, "Unexpected queue failure"),
      name: item.file.name,
      state: "failed",
    },
  });

  const runItem = async (version: number, index: number): Promise<void> => {
    const run = state.latestRun;
    const item = run?.items[index];
    if (
      version !== runVersion ||
      run === null ||
      item === undefined ||
      state.phase !== "running"
    ) {
      return;
    }

    const startedAtMs = now();
    const activeItem: ScriptQueueRunItem = { ...item, state: "active" };
    patchState({
      currentIndex: index,
      latestRun: {
        ...run,
        items: run.items.map((current, itemIndex) =>
          itemIndex === index ? activeItem : current,
        ),
      },
    });

    let outcome: ScriptRunTerminalOutcome;
    try {
      const session = await dependencies.startScript(
        item.file,
        item.inputValues,
      );
      outcome = await session.terminal;
    } catch (cause) {
      dependencies.onUnexpectedError(cause);
      outcome = unexpectedFailure(item, cause);
    }

    const currentRun = applyTerminalOutcome(
      version,
      index,
      outcome,
      startedAtMs,
    );
    if (currentRun === null) return;

    if (getState().phase === "stopping") {
      finishRun(version, "canceled");
      return;
    }

    const hasNext = index + 1 < currentRun.items.length;
    switch (outcome.kind) {
      case "completed":
      case "script-stopped":
        if (hasNext) {
          launchItem(version, index + 1);
        } else {
          finishRun(version, "completed");
        }
        return;
      case "failed":
        if (!hasNext) {
          finishRun(version, "failed");
          return;
        }
        patchState({
          latestRun: { ...currentRun, status: "paused" },
          phase: "paused",
        });
        return;
      case "externally-stopped":
        finishRun(version, "stopped");
        return;
      case "script-exited":
        finishRun(version, "exited");
    }
  };

  const launchItem = (version: number, index: number): void => {
    const settlement = runItem(version, index);
    activeSettlement = settlement;
    void settlement.finally(() => {
      if (activeSettlement === settlement) activeSettlement = null;
    });
  };

  const preparationIsCurrent = (controller: AbortController): boolean =>
    preparationController === controller;

  const abandonPreparation = (
    controller: AbortController,
    error?: string,
    attentionEntryId?: string,
  ): void => {
    if (!preparationIsCurrent(controller)) return;
    preparationController = null;
    patchState({
      ...(attentionEntryId === undefined ? {} : { attentionEntryId }),
      currentIndex: null,
      ...(error === undefined ? {} : { error }),
      phase: "idle",
    });
  };

  const start: ScriptQueue["start"] = async () => {
    if (state.phase !== "idle" || draftController !== null) return false;
    if (state.entries.length === 0) {
      patchState({
        error: "Add at least one script before starting the queue.",
      });
      return false;
    }

    const controller = new AbortController();
    preparationController = controller;
    patchState({
      attentionEntryId: undefined,
      currentIndex: null,
      error: undefined,
      phase: "preparing",
    });
    const entries = state.entries;

    try {
      const resolvedEntries = await Promise.all(
        entries.map(async (entry) => {
          try {
            return {
              entry,
              file: await dependencies.resolve(entry.file),
            };
          } catch (cause) {
            throw new ScriptQueuePreparationError(
              errorMessage(cause, `Failed to resolve ${entry.file.name}.`),
              entry.id,
            );
          }
        }),
      );
      if (!preparationIsCurrent(controller)) return false;

      const items: ScriptQueueRunItem[] = [];
      for (const { entry, file } of resolvedEntries) {
        const values = await requestValidatedInputs(
          file,
          entry.inputValues,
          "preflight",
          controller.signal,
          entry.id,
        );
        if (!preparationIsCurrent(controller)) return false;
        if (values === null) {
          abandonPreparation(controller, undefined, entry.id);
          return false;
        }

        const configuredEntry = queueEntryFor(entry.id, file, values);
        replaceEntry(configuredEntry);
        items.push({
          entryId: entry.id,
          file,
          inputValues: { ...values },
          state: "pending",
        });
      }

      if (await dependencies.isRunnerActive()) {
        if (!preparationIsCurrent(controller)) return false;
        const confirmed = await dependencies.confirmStandaloneReplacement(
          controller.signal,
        );
        if (!preparationIsCurrent(controller)) return false;
        if (!confirmed) {
          abandonPreparation(controller);
          return false;
        }
        await dependencies.stopScript("Replaced by script queue");
      }
      if (!preparationIsCurrent(controller)) return false;

      preparationController = null;
      const version = ++runVersion;
      patchState({
        attentionEntryId: undefined,
        error: undefined,
        latestRun: {
          items,
          status: "running",
        },
        phase: "running",
      });
      launchItem(version, 0);
      return true;
    } catch (cause) {
      if (!preparationIsCurrent(controller)) return false;
      const preparationError =
        cause instanceof ScriptQueuePreparationError ? cause : undefined;
      abandonPreparation(
        controller,
        errorMessage(cause, "Failed to prepare the script queue."),
        preparationError?.entryId,
      );
      return false;
    }
  };

  const cancel: ScriptQueue["cancel"] = async (reason = "Queue canceled") => {
    if (state.phase === "idle") return;
    if (state.phase === "preparing") {
      preparationController?.abort();
      preparationController = null;
      patchState({ currentIndex: null, phase: "idle" });
      return;
    }

    if (state.phase === "paused") {
      const run = state.latestRun;
      if (run !== null) {
        patchState({
          currentIndex: null,
          latestRun: {
            ...run,
            status: "canceled",
          },
          phase: "idle",
        });
      }
      return;
    }
    if (state.phase === "stopping") {
      if (activeSettlement !== null) await activeSettlement;
      return;
    }

    const run = state.latestRun;
    patchState({
      ...(run === null
        ? {}
        : { latestRun: { ...run, status: "canceled" as const } }),
      phase: "stopping",
    });
    await dependencies.stopScript(reason);
    if (activeSettlement !== null) await activeSettlement;
  };

  const runNext = (): void => {
    const run = state.latestRun;
    const currentIndex = state.currentIndex;
    if (
      state.phase !== "paused" ||
      run === null ||
      currentIndex === null ||
      currentIndex + 1 >= run.items.length
    ) {
      return;
    }
    patchState({
      latestRun: { ...run, status: "running" },
      phase: "running",
    });
    launchItem(runVersion, currentIndex + 1);
  };

  return {
    add,
    cancel,
    clear: () => {
      if (state.phase !== "idle") return;
      // A pending input prompt must not refill the queue after it is cleared.
      draftController?.abort();
      draftController = null;
      patchState({
        attentionEntryId: undefined,
        currentIndex: null,
        entries: [],
        error: undefined,
        latestRun: null,
      });
    },
    dispose: () => {
      if (disposed) return;
      draftController?.abort();
      draftController = null;
      preparationController?.abort();
      preparationController = null;
      listeners.clear();
      void cancel("Renderer closed");
      disposed = true;
    },
    editInputs,
    getState,
    move: (entryId, offset) => {
      if (state.phase !== "idle") return;
      const entries = [...state.entries];
      const index = entries.findIndex((entry) => entry.id === entryId);
      const nextIndex = index + offset;
      if (index < 0 || nextIndex < 0 || nextIndex >= entries.length) return;
      const [entry] = entries.splice(index, 1);
      if (entry === undefined) return;
      entries.splice(nextIndex, 0, entry);
      patchState({ entries });
    },
    onState: (listener) => {
      listeners.add(listener);
      listener(state);
      return () => listeners.delete(listener);
    },
    remove: (entryId) => {
      if (state.phase !== "idle") return;
      const removesAttention = state.attentionEntryId === entryId;
      patchState({
        attentionEntryId: removesAttention ? undefined : state.attentionEntryId,
        entries: state.entries.filter((entry) => entry.id !== entryId),
        ...(removesAttention ? { error: undefined } : {}),
      });
    },
    runNext,
    start,
  };
};
