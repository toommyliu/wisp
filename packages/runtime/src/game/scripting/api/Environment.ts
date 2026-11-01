import {
  environmentItemRulesToDropPolicy,
  type EnvironmentAutomationCapability,
  type EnvironmentState,
} from "@lucent/schemas/environment";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";

import type { EnvironmentShape } from "../../environment/Environment";
import type {
  EnvironmentError,
  EnvironmentSnapshot,
  ScriptEnvironmentApi,
} from "@lucent/scripting-api/contract";

class ScriptEnvironmentError extends Schema.TaggedError<ScriptEnvironmentError>()(
  "EnvironmentError",
  {
    cause: Schema.Defect(),
    operation: Schema.String,
  },
) {
  override get message(): string {
    return `Environment ${this.operation} failed.`;
  }
}

const toEnvironmentSnapshot = (state: EnvironmentState): EnvironmentSnapshot =>
  Object.freeze({
    automation: Object.freeze({ ...state.automation }),
    boosts: Object.freeze([...state.boosts]),
    dropPolicy: Object.freeze(
      environmentItemRulesToDropPolicy(state.itemRules),
    ),
    itemNames: Object.freeze([...state.itemNames]),
    itemNotificationNames: Object.freeze([...state.itemNotificationNames]),
    questAutoRegister: Object.freeze({ ...state.questAutoRegister }),
    questIds: Object.freeze([...state.questIds]),
    questRewards: Object.freeze({ ...state.questRewards }),
  });

const exposeMutation = (
  operation: Exclude<keyof ScriptEnvironmentApi, "getState">,
  effect: Effect.Effect<EnvironmentState, unknown>,
): Effect.Effect<EnvironmentSnapshot, EnvironmentError> =>
  effect.pipe(
    Effect.map(toEnvironmentSnapshot),
    Effect.mapError(
      (cause) => new ScriptEnvironmentError({ cause, operation }),
    ),
  );

export const makeScriptEnvironmentApi = (
  environment: EnvironmentShape,
): ScriptEnvironmentApi => {
  const automationSetters = {
    boosts: environment.setBoostAutomationEnabled,
    drops: environment.setDropAutomationEnabled,
    quests: environment.setQuestAutomationEnabled,
  } satisfies Record<
    EnvironmentAutomationCapability,
    (enabled: boolean) => Effect.Effect<EnvironmentState, unknown>
  >;

  const api: ScriptEnvironmentApi = {
    getState: () =>
      environment.getState().pipe(Effect.map(toEnvironmentSnapshot)),
    clearRegistrations: () =>
      exposeMutation("clearRegistrations", environment.clear()),
    setAutomationEnabled: (capability, enabled) =>
      exposeMutation(
        "setAutomationEnabled",
        automationSetters[capability](enabled),
      ),

    addQuest: (questId, rewardItemId) =>
      exposeMutation("addQuest", environment.addQuest(questId, rewardItemId)),
    removeQuest: (questId) =>
      exposeMutation("removeQuest", environment.removeQuest(questId)),
    setQuestReward: (questId, rewardItemId) =>
      exposeMutation(
        "setQuestReward",
        environment.setQuestReward(questId, rewardItemId),
      ),
    clearQuestReward: (questId) =>
      exposeMutation("clearQuestReward", environment.clearQuestReward(questId)),
    clearQuests: () => exposeMutation("clearQuests", environment.clearQuests()),
    updateQuestAutoRegister: (patch) =>
      exposeMutation(
        "updateQuestAutoRegister",
        environment.setQuestAutoRegister(patch),
      ),

    addItem: (name) => exposeMutation("addItem", environment.addItem(name)),
    removeItem: (name) =>
      exposeMutation("removeItem", environment.removeItem(name)),
    clearItems: () => exposeMutation("clearItems", environment.clearItems()),
    setItemNotification: (name, enabled) =>
      exposeMutation(
        "setItemNotification",
        environment.setItemNotification(name, enabled),
      ),
    updateDropPolicy: (patch) =>
      exposeMutation("updateDropPolicy", environment.setDropPolicy(patch)),

    addBoost: (name) => exposeMutation("addBoost", environment.addBoost(name)),
    removeBoost: (name) =>
      exposeMutation("removeBoost", environment.removeBoost(name)),
    clearBoosts: () => exposeMutation("clearBoosts", environment.clearBoosts()),
  };
  return Object.freeze(api);
};
