import type { EnvironmentEdit, EnvironmentState } from "@lucent/game/protocol";
import * as Effect from "effect/Effect";
import type { EnvironmentDropPolicy, EnvironmentItemRules, EnvironmentQuestAutoRegisterOptions } from "@lucent/schemas/environment";

export interface EnvironmentShape {
  getState(): Effect.Effect<EnvironmentState>;
  clear(): Effect.Effect<EnvironmentState, unknown>;
  addQuest(
    questId: number,
    rewardItemId?: number,
  ): Effect.Effect<EnvironmentState, unknown>;
  removeQuest(questId: number): Effect.Effect<EnvironmentState, unknown>;
  setQuestReward(
    questId: number,
    rewardItemId: number,
  ): Effect.Effect<EnvironmentState, unknown>;
  clearQuestReward(questId: number): Effect.Effect<EnvironmentState, unknown>;
  clearQuests(): Effect.Effect<EnvironmentState, unknown>;
  setQuestAutoRegister(
    options: Partial<EnvironmentQuestAutoRegisterOptions>,
  ): Effect.Effect<EnvironmentState, unknown>;
  setAutoRegisterRequirements(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setAutoRegisterRewards(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  addItem(name: string): Effect.Effect<EnvironmentState, unknown>;
  removeItem(name: string): Effect.Effect<EnvironmentState, unknown>;
  setAcceptAcMemberOnlyDrops(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setAcceptAcNonMemberDrops(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setAcceptNonAcMemberOnlyDrops(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setAcceptNonAcNonMemberDrops(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setRejectUnregisteredDrops(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setDropPolicy(
    policy: Partial<EnvironmentDropPolicy>,
  ): Effect.Effect<EnvironmentState, unknown>;
  setItemRules(
    rules: EnvironmentItemRules,
  ): Effect.Effect<EnvironmentState, unknown>;
  clearItems(): Effect.Effect<EnvironmentState, unknown>;
  addBoost(name: string): Effect.Effect<EnvironmentState, unknown>;
  removeBoost(name: string): Effect.Effect<EnvironmentState, unknown>;
  clearBoosts(): Effect.Effect<EnvironmentState, unknown>;
  fetchBoosts(): Effect.Effect<readonly string[], unknown>;
  setBoostAutomationEnabled(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setDropAutomationEnabled(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setItemNotification(
    name: string,
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  setQuestAutomationEnabled(
    enabled: boolean,
  ): Effect.Effect<EnvironmentState, unknown>;
  syncToAll(): Effect.Effect<EnvironmentState, unknown>;
}


export const makeEnvironmentService = (
  host: Pick<typeof __host, "read" | "command">,
  unavailable: Pick<EnvironmentShape, "fetchBoosts" | "syncToAll">,
): EnvironmentShape => {
  const identifier = (value: number) => Number.isFinite(value) ? value : 0;
  const getState = () => Effect.sync(() => JSON.parse(host.read('{"kind":"environment"}')) as EnvironmentState);
  const set = (edit: EnvironmentEdit) => Effect.try(() => JSON.parse(host.command(JSON.stringify({ kind: "updateEnvironment", edit }))) as EnvironmentState);
  const setAutomation = (capability: "boosts" | "drops" | "quests", enabled: boolean) => set({ kind: "setAutomationEnabled", capability, enabled });
  const setDropPolicy: EnvironmentShape["setDropPolicy"] = patch => set({ kind: "updateDropPolicy", patch });
  const setQuestAutoRegister: EnvironmentShape["setQuestAutoRegister"] = options => set({ kind: "setQuestAutoRegister", ...options });
  return {
    ...unavailable,
    getState,
    clear: () => set({ kind: "clear" }),
    addQuest: (questId, rewardItemId) => set({ kind: "addQuest", questId: identifier(questId), ...(rewardItemId === undefined ? {} : { rewardItemId: identifier(rewardItemId) }) }),
    removeQuest: questId => set({ kind: "removeQuest", questId: identifier(questId) }),
    setQuestReward: (questId, rewardItemId) => set({ kind: "setQuestReward", questId: identifier(questId), rewardItemId: identifier(rewardItemId) }),
    clearQuestReward: questId => set({ kind: "clearQuestReward", questId: identifier(questId) }),
    clearQuests: () => set({ kind: "clearQuests" }),
    setQuestAutoRegister,
    setAutoRegisterRequirements: requirements => setQuestAutoRegister({ requirements }),
    setAutoRegisterRewards: rewards => setQuestAutoRegister({ rewards }),
    setBoostAutomationEnabled: enabled => setAutomation("boosts", enabled),
    setDropAutomationEnabled: enabled => setAutomation("drops", enabled),
    setQuestAutomationEnabled: enabled => setAutomation("quests", enabled),
    addItem: name => set({ kind: "addItem", name }),
    removeItem: name => set({ kind: "removeItem", name }),
    clearItems: () => set({ kind: "clearItems" }),
    setItemNotification: (name, enabled) => set({ kind: "setItemNotification", name, enabled }),
    setItemRules: rules => set({ kind: "setItemRules", rules: { ...rules, buckets: [...rules.buckets] } }),
    setDropPolicy,
    setAcceptAcMemberOnlyDrops: enabled => setDropPolicy({ acceptAcMemberOnlyDrops: enabled }),
    setAcceptAcNonMemberDrops: enabled => setDropPolicy({ acceptAcNonMemberDrops: enabled }),
    setAcceptNonAcMemberOnlyDrops: enabled => setDropPolicy({ acceptNonAcMemberOnlyDrops: enabled }),
    setAcceptNonAcNonMemberDrops: enabled => setDropPolicy({ acceptNonAcNonMemberDrops: enabled }),
    setRejectUnregisteredDrops: enabled => setDropPolicy({ rejectUnregisteredDrops: enabled }),
    addBoost: name => set({ kind: "addBoost", name }),
    removeBoost: name => set({ kind: "removeBoost", name }),
    clearBoosts: () => set({ kind: "clearBoosts" }),
  };
};
