import type { Operation, OperationAnswer } from "@lucent/game/protocol";
import * as Effect from "effect/Effect";

const operations = new Map<
  string,
  {
    complete: (answer: OperationAnswer | null, error: string | null) => void;
    fail: (cause: Error) => void;
    timeout?: ReturnType<typeof setTimeout> | undefined;
    started?: boolean;
    requiresIssuance: boolean;
    completionTimeoutMs: number;
  }
>();

export const operate = (
  operation: Operation,
  host: Pick<typeof __host, "prepareOperation" | "operate" | "cancelOperation">,
  queueDeadline = Date.now() + 5_000,
): Effect.Effect<OperationAnswer, Error> =>
  Effect.callback((resume) => {
    let id: string;
    let json: string;
    try {
      json = JSON.stringify(operation);
      id = host.prepareOperation();
    } catch (cause) {
      resume(Effect.fail(cause instanceof Error ? cause : new Error(String(cause))));
      return;
    }
    const cancel = () => {
      const pending = operations.get(id);
      if (pending === undefined) return;
      operations.delete(id);
      clearTimeout(pending.timeout);
      host.cancelOperation(id);
    };
    const complete = (answer: OperationAnswer | null, error: string | null) => {
      const pending = operations.get(id);
      if (pending === undefined) return;
      operations.delete(id);
      clearTimeout(pending.timeout);
      resume(answer === null
        ? Effect.fail(new Error(error ?? "The host operation failed"))
        : Effect.succeed(answer));
    };
    const fail = (cause: Error) => {
      cancel();
      resume(Effect.fail(cause));
    };
    const requiresIssuance = operation.kind === "equipConsumable" || operation.kind === "prepareConsumableClaim";
    operations.set(id, {
      complete,
      fail,
      requiresIssuance,
      completionTimeoutMs: operation.kind === "loadQuests"
        ? 60_000 * Math.max(1, Math.ceil(operation.ids.length / 30))
        : operation.kind === "openBank" ? 45_000 : 35_000,
      timeout: requiresIssuance
        ? setTimeout(() => fail(new Error("The equip permit timed out")), Math.max(0, queueDeadline - Date.now()))
        : undefined,
    });
    try {
      host.operate(id, json);
    } catch (cause) {
      fail(cause instanceof Error ? cause : new Error(String(cause)));
    }
    return Effect.sync(cancel);
  });

export const setOperationResponsibility = (id: string, responsible: boolean): void => {
  const pending = operations.get(id);
  if (pending === undefined || pending.started) return;
  clearTimeout(pending.timeout);
  pending.timeout = responsible
    ? setTimeout(() => pending.fail(new Error("The equip permit timed out")), 5_000)
    : undefined;
};

export const startOperation = (id: string, issued = false): void => {
  const pending = operations.get(id);
  if (pending === undefined || pending.started || (pending.requiresIssuance && !issued)) return;
  pending.started = true;
  clearTimeout(pending.timeout);
  pending.timeout = setTimeout(() => {
    pending.fail(new Error("The host operation timed out"));
  }, pending.completionTimeoutMs);
};

export const completeOperation = (
  id: string,
  answer: OperationAnswer | null,
  error: string | null,
): void => operations.get(id)?.complete(answer, error);

export const failOperations = (cause: Error): void => {
  for (const pending of operations.values()) {
    pending.fail(cause);
  }
};
