import * as Effect from "effect/Effect";

import type { ApiService } from "../../flash/api/Api";
import type { ScriptSettingsApi, ScriptSettingsPatch } from "@lucent/scripting-api/contract";

export const makeScriptSettingsApi = (
  settings: ApiService["settings"],
): ScriptSettingsApi =>
  Object.freeze({
    update: (patch: ScriptSettingsPatch) =>
      settings.apply({
        ...(patch.animations === undefined
          ? {}
          : { animationsEnabled: patch.animations }),
        ...(patch.antiCounter === undefined
          ? {}
          : { antiCounterEnabled: patch.antiCounter }),
        ...(patch.collisions === undefined
          ? {}
          : { collisionsEnabled: patch.collisions }),
        ...(patch.customGuild === undefined
          ? {}
          : { customGuild: patch.customGuild }),
        ...(patch.customName === undefined
          ? {}
          : { customName: patch.customName }),
        ...(patch.deathAds === undefined
          ? {}
          : { deathAdsVisible: patch.deathAds }),
        ...(patch.enemyMagnet === undefined
          ? {}
          : { enemyMagnetEnabled: patch.enemyMagnet }),
        ...(patch.frameRate === undefined
          ? {}
          : { frameRate: patch.frameRate }),
        ...(patch.infiniteRange === undefined
          ? {}
          : { infiniteRangeEnabled: patch.infiniteRange }),
        ...(patch.hidePlayers === undefined
          ? {}
          : { otherPlayersVisible: !patch.hidePlayers }),
        ...(patch.provokeCell === undefined
          ? {}
          : { provokeCellEnabled: patch.provokeCell }),
        ...(patch.renderingMode === undefined
          ? {}
          : { renderingMode: patch.renderingMode }),
        ...(patch.skipCutscenes === undefined
          ? {}
          : { skipCutscenesEnabled: patch.skipCutscenes }),
        ...(patch.walkSpeed === undefined
          ? {}
          : { walkSpeed: patch.walkSpeed }),
      }),
    get: Effect.fn("ScriptSettings.get")(function* () {
      const state = yield* settings.get();
      return Object.freeze({
        animations: state.animationsEnabled,
        antiCounter: state.antiCounterEnabled,
        collisions: state.collisionsEnabled,
        customGuild: state.customGuildConfigured ? state.customGuild : null,
        customName: state.customNameConfigured ? state.customName : null,
        deathAds: state.deathAdsVisible,
        enemyMagnet: state.enemyMagnetEnabled,
        frameRate: state.frameRate,
        infiniteRange: state.infiniteRangeEnabled,
        hidePlayers: !state.otherPlayersVisible,
        provokeCell: state.provokeCellEnabled,
        renderingMode: state.renderingMode,
        skipCutscenes: state.skipCutscenesEnabled,
        walkSpeed: state.walkSpeed,
      });
    }),
  });
