import { toLiveQuest } from "@lucent/game";
import * as Array from "effect/Array";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { operate } from "../../../host/operations";
import type { BridgeService } from "../bridge/Bridge";
import { WireInt } from "../contract/Coercion";
import type { HostStateService } from "../state/HostState";
import type { Wait } from "./Wait";

const isQuestId = (questId: number): boolean =>
  Number.isSafeInteger(questId) && questId > 0;

export interface CompleteQuestOptions {
  /** Number of turn-ins to complete. Must be finite. Omit to use the maximum currently possible. */
  readonly turnIns?: number;
  /** Preferred reward item. Omit when no specific reward is requested. */
  readonly rewardItemId?: number;
}

export const makeQuests = (
  bridge: BridgeService,
  host: HostStateService,
  wait: Wait,
) => {
  const operationHost = globalThis.__host;
  const read = (questId: number) =>
    host
      .read({ kind: "quest", id: questId })
      .pipe(
        Effect.map((quest) => (quest === null ? null : toLiveQuest(quest))),
      );
  const isLoaded = (questId: number) =>
    host
      .read({ kind: "quest", id: questId })
      .pipe(Effect.map((quest) => quest !== null));

  const loadBatch = (questIds: readonly number[], silent = false) => {
    const ids = Array.dedupe(questIds.filter(isQuestId));
    return operate({ kind: "loadQuests", ids, silent }, operationHost).pipe(
      Effect.map((answer) => answer.kind === "loadQuests" ? answer.value : []),
      Effect.catch(() => Effect.forEach(ids, isLoaded)),
    );
  };

  const load = (questId: number, silent = false) =>
    loadBatch([questId], silent).pipe(
      Effect.map((loaded) => loaded[0] ?? false),
    );

  const isInProgress = (questId: number) => {
    if (!isQuestId(questId)) return Effect.succeed(false);
    return bridge
      .invoke("quests.isInProgress", [questId], Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));
  };

  const canComplete = (questId: number) => {
    if (!isQuestId(questId)) return Effect.succeed(false);
    return bridge
      .invoke("quests.canComplete", [questId], Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));
  };

  const accept = (questId: number, silent = false) => {
    if (!isQuestId(questId)) return Effect.succeed(false);
    return Effect.gen(function* () {
      let actionAvailable = false;
      if (!(yield* isLoaded(questId))) {
        actionAvailable = yield* wait.forGameAction("acceptQuest");
        if (!actionAvailable || !(yield* load(questId, silent))) return false;
      }
      if (yield* isInProgress(questId)) return true;
      if (!actionAvailable && !(yield* wait.forGameAction("acceptQuest")))
        return false;
      const accepted = yield* bridge
        .invoke("quests.accept", [questId, silent], Schema.Boolean)
        .pipe(Effect.map(Option.getOrElse(() => false)));
      return accepted
        ? yield* wait.until(isInProgress(questId), {
            timeout: "5 seconds",
          })
        : false;
    });
  };

  const abandon = (questId: number) => {
    if (!isQuestId(questId)) return Effect.succeed(false);
    return Effect.gen(function* () {
      if (!(yield* isInProgress(questId))) return false;
      if (
        Option.isNone(
          yield* bridge.invoke("quests.abandon", [questId], Schema.Void),
        )
      ) {
        return false;
      }
      return yield* wait.until(
        isInProgress(questId).pipe(Effect.map((active) => !active)),
        { timeout: "5 seconds" },
      );
    });
  };

  const acceptBatch = (questIds: readonly number[], silent = false) =>
    Effect.forEach(questIds, (questId) => accept(questId, silent), {
      concurrency: 1,
    });

  const getMaxTurnIns = (questId: number) => {
    if (!isQuestId(questId)) return Effect.succeed(1);
    return bridge.invoke("quests.getMaxTurnIns", [questId], WireInt).pipe(
      Effect.map(
        Option.match({
          onNone: () => 1,
          onSome: (turnIns) => Math.max(1, turnIns),
        }),
      ),
    );
  };

  const complete = (questId: number, options?: CompleteQuestOptions) => {
    if (!isQuestId(questId)) return Effect.succeed(false);
    if (options?.turnIns !== undefined && !Number.isFinite(options.turnIns)) {
      return Effect.succeed(false);
    }
    return Effect.gen(function* () {
      if (!(yield* isInProgress(questId))) return false;
      if (!(yield* canComplete(questId))) return false;
      if (
        !(yield* wait.forGameAction("tryQuestComplete", {
          timeout: "5 seconds",
        }))
      ) {
        return false;
      }
      const turnIns =
        options?.turnIns === undefined
          ? yield* getMaxTurnIns(questId)
          : Math.max(1, Math.trunc(options.turnIns));
      const event = yield* wait.forEvent(
        { questId, type: "quest-complete" },
        {
          timeout: "5 seconds",
          trigger: bridge
            .invoke(
              "quests.complete",
              [questId, turnIns, options?.rewardItemId ?? -1, false],
              Schema.Void,
            )
            .pipe(Effect.map(Option.isSome)),
        },
      );
      return event !== null;
    });
  };

  const get = (questId: number) => {
    if (!isQuestId(questId)) return Effect.succeed(null);
    return read(questId).pipe(
      Effect.flatMap((quest) =>
        quest !== null
          ? Effect.succeed(quest)
          : load(questId).pipe(Effect.andThen(read(questId))),
      ),
    );
  };

  const getAccepted = () =>
    host.command({ kind: "refreshAcceptedQuests" }).pipe(
      Effect.flatMap(
        Option.match({
          onNone: () => host.read({ kind: "acceptedQuests" }),
          onSome: Effect.succeed,
        }),
      ),
      Effect.map((quests) => quests.map(toLiveQuest)),
    );

  const getAll = () =>
    host
      .read({ kind: "quests" })
      .pipe(Effect.map((quests) => quests.map(toLiveQuest)));

  const isAvailable = (questId: number) => {
    if (!isQuestId(questId)) return Effect.succeed(false);
    return bridge
      .invoke("quests.isAvailable", [questId], Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));
  };

  return {
    abandon,
    accept,
    acceptBatch,
    canComplete,
    complete,
    get,
    getAccepted,
    getAll,
    getMaxTurnIns,
    isAvailable,
    isInProgress,
    load,
    loadBatch,
  };
};

export type Quests = ReturnType<typeof makeQuests>;
