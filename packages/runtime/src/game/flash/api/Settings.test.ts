import type { Command } from "@lucent/game/protocol";
import { describe, expect, it } from "@effect/vitest";
import * as Effect from "effect/Effect";

import { makeScriptSettingsApi } from "../../scripting/api/Settings";
import { makeFakeHost, makeFakeHostState } from "../testing/FakeHost";
import { makeSettings } from "./Settings";

describe("Settings", () => {
  it.effect("maps script patches to host commands", () =>
    Effect.gen(function* () {
      const fake = makeFakeHost(makeFakeHostState(), { swf: {} } as Window);
      const commands: Command[] = [];
      const settings = makeSettings({
        ...fake,
        command: (command) => {
          commands.push(command);
          return fake.command(command);
        },
      });
      const api = makeScriptSettingsApi(settings);
      yield* api.update({
        animations: false, antiCounter: false, collisions: false,
        customGuild: "Lucent", customName: "Tommy", deathAds: false,
        enemyMagnet: true, frameRate: 12, infiniteRange: true,
        hidePlayers: true, provokeCell: true, renderingMode: "minimal",
        skipCutscenes: true, walkSpeed: 20,
      });
      yield* api.update({ customName: null, customGuild: null });
      yield* api.update({ walkSpeed: 30 });
      expect(commands).toEqual([
        { kind: "updateSettings", patch: {
          animationsEnabled: false, antiCounterEnabled: false, collisionsEnabled: false,
          customGuild: "Lucent", customName: "Tommy", deathAdsVisible: false,
          enemyMagnetEnabled: true, frameRate: 12, infiniteRangeEnabled: true,
          otherPlayersVisible: false, provokeCellEnabled: true, renderingMode: "minimal",
          skipCutscenesEnabled: true, walkSpeed: 20,
        } },
        { kind: "updateSettings", patch: { customName: null, customGuild: null } },
        { kind: "updateSettings", patch: { walkSpeed: 30 } },
      ]);
    }),
  );

  it.effect("maps host snapshots to immutable script settings", () =>
    Effect.gen(function* () {
      const state = makeFakeHostState();
      state.settings = {
        animationsEnabled: false, antiCounterEnabled: false, collisionsEnabled: false,
        customGuild: "Lucent", customGuildConfigured: true,
        customName: "Tommy", customNameConfigured: true, deathAdsVisible: false,
        enemyMagnetEnabled: true, frameRate: 12, infiniteRangeEnabled: true,
        otherPlayersVisible: false, provokeCellEnabled: true, renderingMode: "minimal",
        skipCutscenesEnabled: true, walkSpeed: 20,
      };
      const settings = makeSettings(makeFakeHost(state, { swf: {} } as Window));
      const api = makeScriptSettingsApi(settings);
      const snapshot = yield* api.get();
      expect(snapshot).toEqual({
        animations: false, antiCounter: false, collisions: false,
        customGuild: "Lucent", customName: "Tommy", deathAds: false,
        enemyMagnet: true, frameRate: 12, infiniteRange: true,
        hidePlayers: true, provokeCell: true, renderingMode: "minimal",
        skipCutscenes: true, walkSpeed: 20,
      });
      expect(Object.isFrozen(snapshot)).toBe(true);
      state.settings.customNameConfigured = false;
      state.settings.customGuildConfigured = false;
      expect(yield* api.get()).toMatchObject({ customName: null, customGuild: null });
    }),
  );
});
