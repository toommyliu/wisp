import * as Effect from "effect/Effect";

import type { RoomPolicy } from "@lucent/schemas/accountSettings";
import type { ApiService } from "../../flash/api/Api";
import {
  applyRoomPolicy,
  roomPolicyAcceptsRoom,
  withRoomNumber,
} from "../../flash/domain/MapTarget";
import type { ScriptPlayerApi, ScriptPlayersApi } from "@lucent/scripting-api/contract";
import { makeScriptPlayersApi } from "./Players";

type ScriptRoomPolicyContext = {
  readonly policy: Effect.Effect<RoomPolicy>;
  readonly map: Pick<
    ApiService["map"],
    "isLoaded" | "getName" | "getRoomNumber"
  >;
};

export const makeScriptPlayerJoinMap = (
  joinMap: ApiService["player"]["joinMap"],
  script: ScriptRoomPolicyContext,
): ApiService["player"]["joinMap"] =>
  Effect.fn("ScriptPlayer.joinMap")(function* (map, options) {
    const policy = yield* script.policy;
    if (policy.kind === "random-private" && (yield* script.map.isLoaded())) {
      const currentName = yield* script.map.getName();
      const currentRoom = yield* script.map.getRoomNumber();
      // Only a bare map name can reuse the current room; explicit room requests
      // must retain their existing destination semantics.
      if (
        map.trim().toLowerCase() === currentName.trim().toLowerCase() &&
        roomPolicyAcceptsRoom(policy, currentRoom)
      ) {
        return yield* joinMap(withRoomNumber(map.trim(), currentRoom), options);
      }
    }
    return yield* joinMap(yield* applyRoomPolicy(map, policy), options);
  });

export const makeScriptPlayerApis = (
  player: ApiService["player"],
  players: ApiService["players"],
  script: ScriptRoomPolicyContext,
): {
  readonly player: ScriptPlayerApi;
  readonly players: ScriptPlayersApi;
} => {
  const auras = Object.freeze({
    get: player.auras.get,
    getAll: player.auras.getAll,
    has: player.auras.has,
  });
  const factions = Object.freeze({
    get: player.factions.get,
    getAll: player.factions.getAll,
  });
  const outfits = Object.freeze({
    equip: player.outfits.equip,
    get: player.outfits.get,
    getAll: player.outfits.getAll,
    wear: player.outfits.wear,
  });
  const playerApi: ScriptPlayerApi = Object.freeze({
    auras,
    factions,
    get: player.get,
    getCell: player.getCell,
    getClassName: player.getClassName,
    getClassRank: player.getClassRank,
    getGender: player.getGender,
    getGold: player.getGold,
    getHp: player.getHp,
    getLevel: player.getLevel,
    getMaxHp: player.getMaxHp,
    getMaxMp: player.getMaxMp,
    getMp: player.getMp,
    getPad: player.getPad,
    getPosition: player.getPosition,
    getState: player.getState,
    goToPlayer: player.goToPlayer,
    hasActiveBoost: player.hasActiveBoost,
    isAfk: player.isAfk,
    isAlive: player.isAlive,
    isMember: player.isMember,
    isReady: player.isReady,
    joinMap: makeScriptPlayerJoinMap(
      (map, options) => player.joinMap(map, options),
      script,
    ),
    jumpToCell: player.jumpToCell,
    outfits,
    rest: player.rest,
    walkTo: player.walkTo,
  });

  return {
    player: playerApi,
    players: makeScriptPlayersApi(players),
  };
};
