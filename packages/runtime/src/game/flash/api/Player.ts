import {
  EntityState,
  LiveFaction,
  LiveOutfit,
  toLivePlayer,
} from "@lucent/game";
import type { BoostType, ItemQuery, LiveItem, Position } from "@lucent/game";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import type { BridgeService } from "../bridge/Bridge";
import {
  NonNegativeWireInt,
  PositiveWireInt,
  WireInt,
} from "../contract/Coercion";
import { randomPrivateRoom } from "../domain/MapTarget";
import { startTravel, travelHost } from "../../../host/travel";
import {
  isHydrated,
  readItems,
  type HostStateService,
} from "../state/HostState";
import type { Auth } from "./Auth";
import type { Inventory } from "./Inventory";
import type { CellPositionOptions, Map } from "./Map";
import type { Wait } from "./Wait";

const FactionPayload = Schema.Struct({
  FactionID: PositiveWireInt,
  iRank: Schema.optionalKey(WireInt),
  iRep: Schema.optionalKey(WireInt),
  sName: Schema.String,
});
const FactionPayloads = Schema.Array(FactionPayload);
const OutfitPayload = Schema.Struct({
  None: Schema.optionalKey(NonNegativeWireInt),
  Weapon: Schema.optionalKey(NonNegativeWireInt),
  ar: Schema.optionalKey(NonNegativeWireInt),
  ba: Schema.optionalKey(NonNegativeWireInt),
  co: Schema.optionalKey(NonNegativeWireInt),
  colors: Schema.optionalKey(
    Schema.Struct({
      accessory: Schema.optionalKey(WireInt),
      base: Schema.optionalKey(WireInt),
      eye: Schema.optionalKey(WireInt),
      hair: Schema.optionalKey(WireInt),
      skin: Schema.optionalKey(WireInt),
      trim: Schema.optionalKey(WireInt),
    }),
  ),
  he: Schema.optionalKey(NonNegativeWireInt),
  mi: Schema.optionalKey(NonNegativeWireInt),
  name: Schema.String,
  pe: Schema.optionalKey(NonNegativeWireInt),
});
const OutfitPayloads = Schema.Array(OutfitPayload);

const sameText = (left: string, right: string): boolean =>
  left.localeCompare(right, undefined, { sensitivity: "accent" }) === 0;

const findClassItem = (
  items: readonly LiveItem[],
  query?: ItemQuery,
): LiveItem | undefined =>
  items.find(
    (item) =>
      item.classItem &&
      (query === undefined ? item.equipped : item.matches(query)),
  );

const toFaction = (payload: typeof FactionPayload.Type): LiveFaction =>
  new LiveFaction({
    id: payload.FactionID,
    name: payload.sName,
    rank: payload.iRank ?? 0,
    reputation: payload.iRep ?? 0,
  });

const equippedItemId = (value: number | undefined): number | undefined =>
  value === undefined || value === 0 ? undefined : value;


export interface WalkToOptions {
  /** Movement speed override. */
  readonly speed?: number;
}

export interface RestOptions {
  /** Whether to wait until both HP and MP are full. */
  readonly waitUntilFull?: boolean;
}

const toOutfit = (payload: typeof OutfitPayload.Type): LiveOutfit =>
  new LiveOutfit({
    colors: {
      accessory: payload.colors?.accessory,
      base: payload.colors?.base,
      eye: payload.colors?.eye,
      hair: payload.colors?.hair,
      skin: payload.colors?.skin,
      trim: payload.colors?.trim,
    },
    equipment: {
      armorItemId: equippedItemId(payload.co),
      capeItemId: equippedItemId(payload.ba),
      classItemId: equippedItemId(payload.ar),
      helmItemId: equippedItemId(payload.he),
      itemId: equippedItemId(payload.None),
      miscItemId: equippedItemId(payload.mi),
      petItemId: equippedItemId(payload.pe),
      weaponItemId: equippedItemId(payload.Weapon),
    },
    name: payload.name,
  });

export const makePlayer = (
  bridge: BridgeService,
  host: HostStateService,
  auth: Auth,
  inventory: Inventory,
  map: Map,
  wait: Wait,
) => {
  const read = <A>(
    method: keyof Window["swf"],
    schema: Schema.Decoder<A>,
    fallback: A,
  ) =>
    bridge
      .invoke(method, undefined, schema)
      .pipe(Effect.map(Option.getOrElse(() => fallback)));
  const get = () =>
    host
      .read({ kind: "me" })
      .pipe(
        Effect.map((player) => (player === null ? null : toLivePlayer(player))),
      );
  const readLocation = (
    projected: string,
    method: "player.getCell" | "player.getPad",
  ) =>
    projected !== ""
      ? Effect.succeed(projected)
      : map
          .isLoaded()
          .pipe(
            Effect.flatMap((loaded) =>
              loaded ? read(method, Schema.String, "") : Effect.succeed(""),
            ),
          );
  const getAuras = (options?: { kind?: "active" | "passive" }) =>
    get().pipe(
      Effect.map((player) =>
        (player?.auras ?? []).filter(
          (aura) => options?.kind === undefined || aura.kind === options.kind,
        ),
      ),
    );

  const factionCache = new Map<number, LiveFaction>();
  const outfitCache = new Map<string, LiveOutfit>();

  const getAura = (name: string, options?: { kind?: "active" | "passive" }) =>
    getAuras(options).pipe(
      Effect.map(
        (auras) => auras.find((aura) => sameText(aura.name, name)) ?? null,
      ),
    );
  const hasAura = (name: string, options?: { kind?: "active" | "passive" }) =>
    getAura(name, options).pipe(Effect.map((aura) => aura !== null));
  const auras = { get: getAura, getAll: getAuras, has: hasAura };

  const getFactions = () =>
    bridge.invoke("player.getFactions", undefined, FactionPayloads).pipe(
      Effect.map(
        Option.match({
          onNone: () => Array.from(factionCache.values()),
          onSome: (payloads) => {
            const incoming = payloads.map(toFaction);
            const ids = new Set(incoming.map((faction) => faction.id));
            for (const id of factionCache.keys()) {
              if (!ids.has(id)) factionCache.delete(id);
            }
            for (const faction of incoming) {
              const current = factionCache.get(faction.id);
              if (current !== undefined) current.replaceFrom(faction);
              else factionCache.set(faction.id, faction);
            }
            return Array.from(factionCache.values());
          },
        }),
      ),
    );
  const getFaction = (selector: string | number) =>
    getFactions().pipe(
      Effect.map(
        (factions) =>
          factions.find((faction) =>
            typeof selector === "number"
              ? faction.id === selector
              : sameText(faction.name, selector),
          ) ?? null,
      ),
    );
  const factions = { get: getFaction, getAll: getFactions };

  const getOutfits = () =>
    bridge.invoke("outfits.getAll", undefined, OutfitPayloads).pipe(
      Effect.map(
        Option.match({
          onNone: () => Array.from(outfitCache.values()),
          onSome: (payloads) => {
            const incoming = payloads.map(toOutfit);
            const keys = new Set(
              incoming.map((outfit) => outfit.name.toLowerCase()),
            );
            for (const key of outfitCache.keys()) {
              if (!keys.has(key)) outfitCache.delete(key);
            }
            for (const outfit of incoming) {
              const key = outfit.name.toLowerCase();
              const current = outfitCache.get(key);
              if (current !== undefined) current.replaceFrom(outfit);
              else outfitCache.set(key, outfit);
            }
            return Array.from(outfitCache.values());
          },
        }),
      ),
    );
  const getOutfit = (name: string) =>
    getOutfits().pipe(
      Effect.map(
        (outfits) =>
          outfits.find((outfit) => sameText(outfit.name, name)) ?? null,
      ),
    );
  const equipOutfit = (name: string, keepColors = false) =>
    wait
      .forGameAction("equipLoadout")
      .pipe(
        Effect.flatMap((ready) =>
          ready
            ? bridge
                .invoke("outfits.equip", [name, keepColors], Schema.Boolean)
                .pipe(Effect.map(Option.getOrElse(() => false)))
            : Effect.succeed(false),
        ),
      );
  const wearOutfit = (name: string, keepColors = false) =>
    wait
      .forGameAction("wearLoadout")
      .pipe(
        Effect.flatMap((ready) =>
          ready
            ? bridge
                .invoke("outfits.wear", [name, keepColors], Schema.Boolean)
                .pipe(Effect.map(Option.getOrElse(() => false)))
            : Effect.succeed(false),
        ),
      );
  const outfits = {
    equip: equipOutfit,
    get: getOutfit,
    getAll: getOutfits,
    wear: wearOutfit,
  };

  const getCell = () =>
    get().pipe(
      Effect.flatMap((current) =>
        readLocation(current?.cell ?? "", "player.getCell"),
      ),
    );
  const getClassName = () => read("player.getClassName", Schema.String, "");
  const getClassRank = Effect.fn("Player.getClassRank")(function* (
    query?: ItemQuery,
  ) {
    const inventoryClass = findClassItem(yield* inventory.getAll(), query);
    if (inventoryClass !== undefined) return inventoryClass.classRank;
    if (query === undefined || !(yield* isHydrated(host, "bank"))) {
      return null;
    }

    const bankClass = findClassItem(yield* readItems(host, "bank"), query);
    return bankClass?.classRank ?? null;
  });
  const getGender = () => read("player.getGender", Schema.String, "");
  const getGold = () => read("player.getGold", WireInt, 0);
  const getHp = () => get().pipe(Effect.map((current) => current?.hp ?? 0));
  const getLevel = () =>
    get().pipe(Effect.map((current) => current?.level ?? 0));
  const getMaxHp = () =>
    get().pipe(Effect.map((current) => current?.maxHp ?? 0));
  const getMaxMp = () =>
    get().pipe(Effect.map((current) => current?.maxMp ?? 0));
  const getMp = () => get().pipe(Effect.map((current) => current?.mp ?? 0));
  const getPad = () =>
    get().pipe(
      Effect.flatMap((current) =>
        readLocation(current?.pad ?? "", "player.getPad"),
      ),
    );
  const getState = () =>
    get().pipe(Effect.map((current) => current?.state ?? EntityState.Idle));
  const getPosition = () =>
    get().pipe(
      Effect.map((current) => ({
        ...(current?.position ?? { x: 0, y: 0 }),
      })),
    );
  const goToPlayer = (name: string) => {
    const username = name.trim();
    return username === ""
      ? Effect.void
      : bridge
          .invoke("player.goToPlayer", [username], Schema.Void)
          .pipe(Effect.asVoid);
  };
  const hasActiveBoost = (boostType: BoostType) =>
    bridge
      .invoke("player.hasActiveBoost", [boostType], Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));
  const isAfk = () =>
    get().pipe(Effect.map((current) => current?.afk ?? false));
  const isAlive = () =>
    get().pipe(
      Effect.flatMap((current) =>
        current !== null
          ? Effect.succeed(current.alive)
          : Effect.all([
              read("player.getHp", WireInt, 0),
              read("player.getState", WireInt, EntityState.Dead),
            ]).pipe(
              Effect.map(([hp, state]) => hp > 0 && state !== EntityState.Dead),
            ),
      ),
    );
  const isMember = () => read("player.isMember", Schema.Boolean, false);
  const isReady = makePlayerReadiness(host, auth, map, bridge);

  const jumpToCell = (cell: string, pad?: string) =>
    startTravel({ kind: "jump", cell, pad }, travelHost);
  const exitCombat = () => startTravel({ kind: "exit" }, travelHost);
  const joinMap = (target: string, options?: CellPositionOptions) =>
    Effect.gen(function* () {
      const random_room = yield* randomPrivateRoom;
      return yield* startTravel({ kind: "join", target, ...options, random_room }, travelHost);
    });

  const rest = (options?: RestOptions) =>
    Effect.gen(function* () {
      const current = yield* get();
      if (
        current !== null &&
        current.hp >= current.maxHp &&
        current.mp >= current.maxMp
      )
        return true;
      if (!(yield* wait.forGameAction("rest"))) return false;
      if (
        Option.isNone(
          yield* bridge.invoke("player.rest", undefined, Schema.Void),
        )
      )
        return false;
      if (options?.waitUntilFull !== true) return true;
      return yield* wait.until(
        get().pipe(
          Effect.map(
            (player) =>
              player !== null &&
              player.hp >= player.maxHp &&
              player.mp >= player.maxMp,
          ),
        ),
        { timeout: "10 seconds" },
      );
    });

  const walkTo = (position: Position, options?: WalkToOptions) =>
    Effect.gen(function* () {
      if (!(yield* isAlive())) return false;
      const targetX = Math.trunc(position.x);
      const targetY = Math.trunc(position.y);
      const speed = options?.speed;
      const args: Parameters<Window["swf"]["player.walkTo"]> =
        speed === undefined ? [targetX, targetY] : [targetX, targetY, speed];
      const started = yield* bridge
        .invoke("player.walkTo", args, Schema.Boolean)
        .pipe(Effect.map(Option.getOrElse(() => false)));
      if (!started) return false;
      return yield* wait.until(
        get().pipe(
          Effect.map(
            (current) =>
              current?.position.x === targetX && current.position.y === targetY,
          ),
        ),
        { timeout: "3 seconds" },
      );
    });

  return {
    auras,
    exitCombat,
    factions,
    get,
    getCell,
    getClassName,
    getClassRank,
    getGender,
    getGold,
    getHp,
    getLevel,
    getMaxHp,
    getMaxMp,
    getMp,
    getPad,
    getPosition,
    getState,
    goToPlayer,
    hasActiveBoost,
    isAfk,
    isAlive,
    isMember,
    isReady,
    joinMap,
    jumpToCell,
    outfits,
    rest,
    walkTo,
  };
};

export type Player = ReturnType<typeof makePlayer>;

export const makePlayerReadiness = (host: HostStateService, auth: Auth, map: Map, bridge: BridgeService) => () =>
    host
      .read({ kind: "readiness" })
      .pipe(
        Effect.flatMap((readiness) =>
          readiness.missing.length === 0
            ? Effect.all([
                auth.isLoggedIn(),
                map.isLoaded(),
                bridge.invoke("player.isLoaded", undefined, Schema.Boolean).pipe(Effect.map(Option.getOrElse(() => false))),
              ]).pipe(
                Effect.map(
                  ([loggedIn, loaded, playerLoaded]) =>
                    loggedIn && loaded && playerLoaded,
                ),
              )
            : Effect.succeed(false),
        ),
      );
