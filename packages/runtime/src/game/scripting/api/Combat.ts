import type { ApiService } from "../../flash/api/Api";
import type { ScriptCombatApi } from "@lucent/scripting-api/contract";

export const makeScriptCombatApi = (
  combat: ApiService["combat"],
): ScriptCombatApi => {
  const killForItem: ScriptCombatApi["killForItem"] = (target, goal, options) =>
    combat.killForItem(target, goal.item, goal.quantity, options);
  const killForTempItem: ScriptCombatApi["killForTempItem"] = (
    target,
    goal,
    options,
  ) => combat.killForTempItem(target, goal.item, goal.quantity, options);
  const target = Object.freeze({
    auras: Object.freeze({
      get: combat.target.auras.get,
      getAll: combat.target.auras.getAll,
      has: combat.target.auras.has,
    }),
    get: combat.target.get,
  });

  return Object.freeze({
    attack: combat.attack,
    cancelAutoAttack: combat.cancelAutoAttack,
    cancelTarget: combat.cancelTarget,
    canUseSkill: combat.canUseSkill,
    exit: combat.exit,
    getSkillCooldownRemainingMs: combat.getSkillCooldownRemainingMs,
    hunt: combat.hunt,
    kill: combat.kill,
    killForItem,
    killForTempItem,
    target,
    useSkill: combat.useSkill,
  });
};
