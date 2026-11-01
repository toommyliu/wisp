import { PositiveInt, TrimmedNonEmptyString } from "@lucent/schemas";
import {
  matchesAppliedEnhancement,
  matchesEnhancementShopItem,
  resolveEnhancementStrategy,
  toItemSelector,
} from "@lucent/game";
import type { Item, ItemQuery } from "@lucent/game";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { EnhancementSelectorSchema } from "../../EnhancementSelectors";
import type { ApiService } from "../../flash/api/Api";
import type { BridgeService } from "../../flash/bridge/Bridge";
import type { Packet } from "../../flash/contract/Packet";
import type { ScriptEnhanceItemOptions, ScriptShopApi } from "@lucent/scripting-api/contract";

interface ScriptShopDependencies {
  readonly inventory: Pick<ApiService["inventory"], "get">;
  readonly player: Pick<
    ApiService["player"],
    "getLevel" | "isMember" | "joinMap"
  >;
  readonly shops: ApiService["shops"];
  readonly wait: Pick<ApiService["wait"], "forGameAction" | "forPacket">;
}

const enhancementResponseContains = (packet: Packet, itemId: number) => {
  if (packet.direction === "client" || typeof packet.data !== "object") {
    return false;
  }
  const itemIds = (packet.data as { readonly ItemIDs?: unknown }).ItemIDs;
  return (
    Array.isArray(itemIds) && itemIds.some((value) => Number(value) === itemId)
  );
};

const candidateOrder = (member: boolean) => (left: Item, right: Item) => {
  const levelDifference =
    (right.enhancement?.level ?? 0) - (left.enhancement?.level ?? 0);
  if (levelDifference !== 0) return levelDifference;
  if (member && left.memberOnly !== right.memberOnly) {
    return (
      Number(right.memberOnly === member) - Number(left.memberOnly === member)
    );
  }
  return (left.shopItemId ?? left.itemId) - (right.shopItemId ?? right.itemId);
};

const selectorForCandidate = (candidate: Item) =>
  candidate.shopItemId === undefined
    ? { itemId: candidate.itemId }
    : { shopItemId: candidate.shopItemId };

const ScriptItemQuerySchema = Schema.Union([
  PositiveInt,
  TrimmedNonEmptyString,
  Schema.Struct({
    itemId: PositiveInt,
    name: Schema.optionalKey(Schema.Never),
  }),
  Schema.Struct({
    itemId: Schema.optionalKey(Schema.Never),
    name: TrimmedNonEmptyString,
  }),
]);

export const ScriptEnhanceItemOptionsSchema =
  EnhancementSelectorSchema satisfies Schema.Schema<ScriptEnhanceItemOptions>;

const ScriptEnhanceItemRequestSchema = Schema.Struct({
  item: ScriptItemQuerySchema,
  options: ScriptEnhanceItemOptionsSchema,
});

const decodeScriptEnhanceItemRequest = Schema.decodeUnknownOption(
  ScriptEnhanceItemRequestSchema,
);

export const makeScriptShopApi = (
  services: ScriptShopDependencies,
  bridge: BridgeService,
): ScriptShopApi => {
  const enhanceItem = Effect.fn("ScriptShop.enhanceItem")(function* (
    item: ItemQuery,
    options: ScriptEnhanceItemOptions,
  ) {
    const itemSelector = toItemSelector(item);
    const { enhancement, special } = options;

    const inventoryItem = yield* services.inventory.get(itemSelector);
    if (inventoryItem === null) {
      return false;
    }

    const playerLevel = yield* services.player.getLevel();
    const strategyResolution = resolveEnhancementStrategy(
      inventoryItem,
      enhancement,
      playerLevel,
      special,
    );
    if (!strategyResolution.ok) {
      return false;
    }
    const strategy = strategyResolution.strategy;

    const matchesApplied = matchesAppliedEnhancement(inventoryItem, strategy);
    const appliedLevel = matchesApplied
      ? inventoryItem.enhancement?.level
      : undefined;
    if (appliedLevel !== undefined && appliedLevel >= playerLevel) {
      return true;
    }

    if (strategy.map !== undefined) {
      const joined = yield* services.player.joinMap(strategy.map);
      if (!joined) return false;
    }

    const loaded = yield* services.shops.load(strategy.shopId);
    if (!loaded) return false;

    const member = yield* services.player.isMember();
    const shopItems = yield* services.shops.getAll();
    const candidates = shopItems
      .filter(
        (candidate) =>
          (!candidate.memberOnly || member) &&
          candidate.enhancement?.level !== undefined &&
          candidate.enhancement.level <= playerLevel &&
          matchesEnhancementShopItem(candidate, strategy),
      )
      .toSorted(candidateOrder(member));

    let candidateSelector: ReturnType<typeof selectorForCandidate> | undefined;
    for (const current of candidates) {
      const candidateLevel = current.enhancement?.level ?? 0;
      if (appliedLevel !== undefined && appliedLevel >= candidateLevel) {
        return true;
      }
      const selector = selectorForCandidate(current);
      const canBuy = yield* services.shops.canBuy(selector);
      if (!canBuy) continue;
      candidateSelector = selector;
      break;
    }
    if (candidateSelector === undefined) {
      return false;
    }

    const actionAvailable = yield* services.wait.forGameAction("buyItem");
    if (!actionAvailable) return false;

    const response = yield* services.wait.forPacket(
      {
        command: "enhanceItemShop",
        direction: "extension",
        predicate: (packet) =>
          enhancementResponseContains(packet, inventoryItem.itemId),
        encoding: "json",
      },
      {
        timeout: "5 seconds",
        trigger: Effect.gen(function* () {
          const bridgeResult = yield* bridge.invoke(
            "shops.enhance",
            [candidateSelector, inventoryItem.itemId],
            Schema.Boolean,
          );
          return Option.getOrElse(bridgeResult, () => false);
        }),
      },
    );
    if (response === null) {
      return false;
    }

    const updated = yield* services.inventory.get(inventoryItem.itemId);
    return updated !== null && matchesAppliedEnhancement(updated, strategy);
  });

  const shop: ScriptShopApi = {
    buy: services.shops.buy,
    canBuy: services.shops.canBuy,
    close: services.shops.close,
    enhanceItem: (item, options) =>
      Option.match(decodeScriptEnhanceItemRequest({ item, options }), {
        onNone: () => Effect.succeed(false),
        onSome: (request) => enhanceItem(request.item, request.options),
      }),
    get: services.shops.get,
    getAll: services.shops.getAll,
    getCurrent: services.shops.getCurrent,
    getMaxBuyQuantity: services.shops.getMaxBuyQuantity,
    isMergeShop: services.shops.isMergeShop,
    isOpen: services.shops.isOpen,
    open: services.shops.load,
    openArmorCustomize: services.shops.openArmorCustomize,
    openHairShop: services.shops.openHairShop,
    sell: services.shops.sell,
  };
  return Object.freeze(shop);
};
