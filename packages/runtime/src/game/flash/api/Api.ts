import * as Context from "effect/Context";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";

import { Bridge } from "../bridge/Bridge";
import { Gateway } from "../bridge/Gateway";
import { HostState } from "../state/HostState";
import { makeAuth } from "./Auth";
import { makeAntiCounter } from "./internal/AntiCounter";
import { makeBank } from "./Bank";
import { makeCombat } from "./Combat";
import { makeDrops } from "./Drops";
import { makeEvents } from "./Events";
import { makeHouse } from "./House";
import { makeInventory } from "./Inventory";
import { makeMap } from "./Map";
import { makeMonsters } from "./Monsters";
import { makePacket } from "./Packet";
import { makePlayer } from "./Player";
import { makePlayers } from "./Players";
import { makeQuests } from "./Quests";
import {
  ScriptReactions,
  start as startScriptReactions,
} from "./ScriptReactions";
import { makeSettings } from "./Settings";
import { makeShops } from "./Shops";
import { makeTempInventory } from "./TempInventory";
import { makeWaitApi } from "./Wait";

const makeApiServices = Effect.gen(function* () {
  const bridge = yield* Bridge;
  const gateway = yield* Gateway;
  const host = yield* HostState;
  const wait = makeWaitApi(bridge, gateway);
  const events = yield* makeEvents(gateway, wait);
  const packet = yield* makePacket(gateway, host, wait);
  const auth = makeAuth(bridge, host, wait);
  const house = makeHouse(bridge, host);
  const inventory = makeInventory(bridge, host, house, wait);
  const bank = yield* makeBank(bridge, host, auth, inventory, house, wait);
  const drops = yield* makeDrops(bridge, host, auth);
  const map = makeMap(bridge, host, wait);
  const monsterServices = yield* makeMonsters(bridge, host, wait);
  const monsters = monsterServices.api;
  const players = makePlayers(host);
  const player = makePlayer(bridge, host, auth, inventory, map, wait);
  const quests = makeQuests(bridge, host, wait);
  const settings = makeSettings(host);
  const antiCounter = makeAntiCounter(bridge, settings.isAntiCounterEnabled);
  const shops = makeShops(bridge, host, inventory, wait);
  const tempInventory = makeTempInventory(host);
  const combat = makeCombat(
    bridge,
    antiCounter,
    host,
    drops,
    events,
    inventory,
    map,
    monsterServices.lookup,
    player,
    players,
    settings,
    tempInventory,
    wait,
  );

  return {
    api: {
      auth,
      bank,
      combat,
      drops,
      events,
      house,
      inventory,
      map,
      monsterLookup: monsterServices.lookup,
      monsters,
      packet,
      player,
      players,
      quests,
      settings,
      shops,
      tempInventory,
      wait,
    },
    scriptReactions: {
      antiCounter,
      events,
      gateway,
      host,
      monsters: monsterServices,
    },
  };
});

export type ApiService = Effect.Success<typeof makeApiServices>["api"];

export const makeApi = makeApiServices.pipe(
  Effect.tap(({ scriptReactions }) => startScriptReactions(scriptReactions)),
  Effect.map(({ api }) => api),
);

export class Api extends Context.Service<Api, ApiService>()(
  "lucent/renderer/flash/Api",
) {}

export const layer = Layer.effectContext(
  makeApiServices.pipe(
    Effect.map(({ api, scriptReactions }) =>
      Context.make(Api, api).pipe(
        Context.add(ScriptReactions, scriptReactions),
      ),
    ),
  ),
);
