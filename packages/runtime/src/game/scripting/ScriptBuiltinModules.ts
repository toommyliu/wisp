import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";

import type { RoomPolicy } from "@lucent/schemas/accountSettings";
import type { BridgeService } from "../flash/bridge/Bridge";
import { makeScriptArmyApi } from "./api/Army";
import { makeScriptCombatApi } from "./api/Combat";
import { makeScriptEventsApi } from "./api/Events";
import { makeScriptEnvironmentApi } from "./api/Environment";
import { makeScriptPacketApi } from "./api/Packet";
import { makeScriptPlayerApis } from "./api/Player";
import { makeScriptPublicServices } from "./api/PublicServices";
import type { ScriptRuntimeServices } from "./api/Services";
import { makeScriptSettingsApi } from "./api/Settings";
import { makeScriptShopApi } from "./api/Shop";
import type {
  ScriptApi,
  ScriptAutoReloginApi,
  ScriptAutoZoneApi,
  ScriptEffectStd,
  ScriptFileSystemApi,
  ScriptHttpApi,
  ScriptRuntimeApi,
} from "@lucent/scripting-api/contract";
import type { ScriptAsyncScope } from "./scriptAsyncScope";
import { scriptEffectStd } from "./ScriptEffectStd";
import { scriptSchema } from "./schema";

export interface ScriptBuiltinModules {
  readonly effect: ScriptEffectStd;
  readonly "lucent/api": ScriptApi;
  readonly "lucent/autorelogin": ScriptAutoReloginApi;
  readonly "lucent/autozone": ScriptAutoZoneApi;
  readonly "lucent/filesystem": ScriptFileSystemApi;
  readonly "lucent/http": ScriptHttpApi;
  readonly "lucent/schema": typeof scriptSchema;
  readonly "lucent/script": ScriptRuntimeApi;
}

export interface ScriptBuiltinModulesOptions {
  readonly autoRelogin: ScriptAutoReloginApi;
  readonly autoZone: ScriptAutoZoneApi;
  readonly bridge: BridgeService;
  readonly failCause: (cause: Cause.Cause<unknown>) => Effect.Effect<void>;
  readonly fileSystem: ScriptFileSystemApi;
  readonly http: ScriptHttpApi;
  readonly roomPolicy: Effect.Effect<RoomPolicy>;
  readonly scope: ScriptAsyncScope;
  readonly script: ScriptRuntimeApi;
  readonly services: ScriptRuntimeServices;
}

const makeScriptAutoReloginApi = (
  api: ScriptAutoReloginApi,
): ScriptAutoReloginApi =>
  Object.freeze({
    getDelayMs: api.getDelayMs,
    getServer: api.getServer,
    getState: api.getState,
    isEnabled: api.isEnabled,
    setDelay: api.setDelay,
    setEnabled: api.setEnabled,
    setServer: api.setServer,
  });

const makeScriptAutoZoneApi = (api: ScriptAutoZoneApi): ScriptAutoZoneApi =>
  Object.freeze({
    getMap: api.getMap,
    getState: api.getState,
    isEnabled: api.isEnabled,
    setEnabled: api.setEnabled,
    setMap: api.setMap,
  });

/** Creates the frozen built-in module registry for one script execution. */
export const makeScriptBuiltinModules = (
  options: ScriptBuiltinModulesOptions,
): ScriptBuiltinModules => {
  const army = makeScriptArmyApi(
    options.services.army,
    options.scope,
    options.failCause,
  );
  const events = makeScriptEventsApi(
    options.services.events,
    options.scope,
    options.failCause,
  );
  const environment = makeScriptEnvironmentApi(options.services.environment);
  const packet = makeScriptPacketApi(
    options.services.packet,
    options.scope,
    options.failCause,
  );
  const { player, players } = makeScriptPlayerApis(
    options.services.player,
    options.services.players,
    { policy: options.roomPolicy, map: options.services.map },
  );
  const settings = makeScriptSettingsApi(options.services.settings);
  const shop = makeScriptShopApi(
    {
      inventory: options.services.inventory,
      player: {
        getLevel: player.getLevel,
        isMember: player.isMember,
        joinMap: player.joinMap,
      },
      shops: options.services.shops,
      wait: options.services.wait,
    },
    options.bridge,
  );
  const combat = makeScriptCombatApi(options.services.combat);
  const publicServices = makeScriptPublicServices(options.services);
  const api: ScriptApi = Object.freeze({
    army,
    auth: publicServices.auth,
    bank: publicServices.bank,
    combat,
    drops: publicServices.drops,
    environment,
    events,
    house: publicServices.house,
    inventory: publicServices.inventory,
    map: publicServices.map,
    monsters: publicServices.monsters,
    packet,
    player,
    players,
    quests: publicServices.quests,
    settings,
    shop,
    tempInventory: publicServices.tempInventory,
    wait: publicServices.wait,
  });

  return Object.freeze({
    effect: scriptEffectStd,
    "lucent/api": api,
    "lucent/autorelogin": makeScriptAutoReloginApi(options.autoRelogin),
    "lucent/autozone": makeScriptAutoZoneApi(options.autoZone),
    "lucent/filesystem": options.fileSystem,
    "lucent/http": options.http,
    "lucent/schema": scriptSchema,
    "lucent/script": options.script,
  });
};
