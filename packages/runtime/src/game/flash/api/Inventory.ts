import {
  matchesAppliedEnhancement,
  normalizeItemQuantity,
  resolveEnhancementStrategy,
} from "@lucent/game";
import type { InventoryPool, ItemQuery, LiveItem } from "@lucent/game";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import * as Semaphore from "effect/Semaphore";

import { operate } from "../../../host/operations";

import {
  EquipEnhancementSelectorSchema,
  type EquipEnhancementSelector,
} from "../../EnhancementSelectors";
import type { BridgeService } from "../bridge/Bridge";
import { PositiveWireInt, WireBoolean, WireInt } from "../contract/Coercion";
import { packetData } from "../contract/Packet";
import {
  isHydrated,
  readItem,
  readItems,
  type HostStateService,
} from "../state/HostState";
import type { House } from "./House";
import type { Wait } from "./Wait";

const WearResponse = Schema.Struct({
  ItemID: PositiveWireInt,
  success: WireBoolean,
  uid: PositiveWireInt,
});
const EquipResponse = Schema.Struct({
  ItemID: PositiveWireInt,
  uid: PositiveWireInt,
});
const decodeWearResponse = Schema.decodeUnknownOption(WearResponse);
const decodeEquipResponse = Schema.decodeUnknownOption(EquipResponse);
const decodeEquipEnhancementSelector = Schema.decodeUnknownOption(
  EquipEnhancementSelectorSchema,
);

export interface EquipOptions {
  /**
   * Whether to wear wearable equipment after equipping it.
   * @defaultValue true
   */
  readonly wear?: boolean;
}

/** Whether AQW consumes the usable item directly instead of assigning slot 6. */
export const isDirectInventoryConsumable = (link: string): boolean => {
  const normalized = link.trim().toLowerCase();
  return normalized === "elixir" || normalized === "tonic";
};

const isDirectInventoryUseItem = (category: string, link: string): boolean =>
  category.trim().toLowerCase() === "serveruse" ||
  isDirectInventoryConsumable(link);

const slotMethods = {
  bag: "inventory.getSlots",
  misc: "inventory.getMiscSlots",
} as const satisfies Record<InventoryPool, keyof Window["swf"]>;

export const makeInventory = (
  bridge: BridgeService,
  host: HostStateService,
  house: House,
  wait: Wait,
) => {
  // Consumable equips and unequips mutate the same client action slot.
  const consumableEquipment = Semaphore.makeUnsafe(1);
  const getAll = () => readItems(host, "inventory");

  const get = (selector: ItemQuery) => readItem(host, "inventory", selector);

  const getSlots = Effect.fn("Inventory.getSlots")(function* (
    pool: InventoryPool = "bag",
  ) {
    if (!(yield* isHydrated(host, "inventory"))) return 0;
    return yield* bridge
      .invoke(slotMethods[pool], undefined, WireInt)
      .pipe(Effect.map(Option.getOrElse(() => 0)));
  });

  const getUsedSlots = (pool: InventoryPool = "bag") =>
    host
      .read({ kind: "items", container: "inventory" })
      .pipe(
        Effect.map(
          (items) => items.filter((item) => item.pool === pool).length,
        ),
      );

  const canAccept = Effect.fn("Inventory.canAccept")(function* (
    item: LiveItem,
    replacing?: LiveItem,
  ) {
    const pool = item.pool;
    if (pool === "class") return true;
    if (
      replacing !== undefined &&
      (pool === "house" || replacing.pool === pool)
    ) {
      return true;
    }
    if (pool === "house") return (yield* house.getAvailableSlots()) > 0;

    const current = yield* get(item.itemId);
    if (
      replacing === undefined &&
      current !== null &&
      current.quantity < (current.maxStack ?? 1)
    ) {
      return true;
    }
    if (pool === "misc" && current !== null) return true;

    return (yield* getAvailableSlots(pool)) > 0;
  });

  const contains = (selector: ItemQuery, quantity?: number) =>
    get(selector).pipe(
      Effect.map(
        (item) =>
          item !== null && item.quantity >= normalizeItemQuantity(quantity),
      ),
    );

  const canUseMemberItem = Effect.fn("Inventory.canUseMemberItem")(function* (
    memberOnly: boolean,
  ) {
    if (!memberOnly) return true;
    return yield* bridge
      .invoke("player.isMember", undefined, Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));
  });

  const getLocalUserId = Effect.fn("Inventory.getLocalUserId")(function* () {
    const cached = yield* host.read({ kind: "selfEntityId" });
    return cached === null
      ? yield* bridge.invoke("player.getUserId", undefined, PositiveWireInt)
      : Option.some(cached);
  });

  const wearItem = Effect.fn("Inventory.wearItem")(function* (itemId: number) {
    const userId = yield* getLocalUserId();
    if (Option.isNone(userId)) return false;
    if (!(yield* wait.forGameAction("wearItem"))) return false;

    let response: typeof WearResponse.Type | undefined;
    const wearPacket = yield* wait.forPacket(
      {
        command: "wearItem",
        direction: "extension",
        predicate: (candidate) => {
          const decoded = decodeWearResponse(packetData(candidate));
          if (
            Option.isNone(decoded) ||
            decoded.value.ItemID !== itemId ||
            decoded.value.uid !== userId.value
          ) {
            return false;
          }
          response = decoded.value;
          return true;
        },
        encoding: "json",
      },
      {
        timeout: "5 seconds",
        trigger: bridge
          .invoke("inventory.wear", [{ itemId }], Schema.Boolean)
          .pipe(Effect.map(Option.getOrElse(() => false))),
      },
    );
    return wearPacket !== null && response?.success === true;
  });

  const wearEffect = Effect.fn("Inventory.wear")(function* (
    selector: ItemQuery,
  ) {
    const item = yield* get(selector);
    if (item === null || !item.wearable) return false;
    if (item.worn) return true;
    if (!(yield* canUseMemberItem(item.memberOnly))) return false;
    return yield* wearItem(item.itemId);
  });
  const wear = (selector: ItemQuery) => wearEffect(selector);

  const operationHost = globalThis.__host;
  const equipConsumable = (itemId: number, claim?: string) =>
    Effect.scoped(Effect.gen(function* () {
      const deadline = Date.now() + 5_000;
      const permit = yield* Effect.acquireRelease(
        consumableEquipment.take(1).pipe(Effect.timeoutOption("5 seconds")),
        (permit) => Option.isSome(permit) ? consumableEquipment.release(1) : Effect.void,
        { interruptible: true },
      );
      if (Option.isNone(permit)) return false;
      return yield* operate(
        claim === undefined
          ? { kind: "equipConsumable", selector: itemId }
          : { kind: "prepareConsumableClaim", claim },
        operationHost,
        deadline,
      ).pipe(
        Effect.map((answer) => answer.kind === "equipConsumable" && answer.value),
        Effect.catch(() => Effect.succeed(false)),
      );
    }));

  const equipEffect = Effect.fn("Inventory.equip")(function* (
    selector: ItemQuery,
    options?: EquipOptions,
    claim?: string,
  ) {
    const item = yield* get(selector);
    if (item === null) return false;
    if (item.category === "Item" && isDirectInventoryConsumable(item.link)) {
      return false;
    }

    // bEquip changes before seia accepts the consumable action, so it cannot
    // establish that an already selected consumable finished equipping.
    const needsEquip = item.category === "Item" || !item.equipped;
    const needsWear = (options?.wear ?? true) && item.wearable && !item.worn;
    if (!needsEquip && !needsWear) return true;
    if (!(yield* canUseMemberItem(item.memberOnly))) return false;

    if (needsEquip) {
      if (!(yield* wait.forGameAction("equipItem"))) return false;
      if (item.category === "Item") {
        const equipped = yield* equipConsumable(item.itemId, claim);
        if (!equipped) return false;
      } else {
        const userId = yield* getLocalUserId();
        if (Option.isNone(userId)) return false;
        const equipPacket = yield* wait.forPacket(
          {
            command: "equipItem",
            direction: "extension",
            predicate: (candidate) => {
              const decoded = decodeEquipResponse(packetData(candidate));
              return (
                Option.isSome(decoded) &&
                decoded.value.ItemID === item.itemId &&
                decoded.value.uid === userId.value
              );
            },
            encoding: "json",
          },
          {
            timeout: "5 seconds",
            trigger: bridge
              .invoke(
                "inventory.equip",
                [{ itemId: item.itemId }],
                Schema.Boolean,
              )
              .pipe(Effect.map(Option.getOrElse(() => false))),
          },
        );
        if (equipPacket === null) return false;
      }
    }

    return needsWear ? yield* wearItem(item.itemId) : true;
  });
  const equip = (selector: ItemQuery, options?: EquipOptions) =>
    equipEffect(selector, options);

  const use = Effect.fn("Inventory.use")(function* (selector: ItemQuery) {
    const item = yield* get(selector);
    if (item === null || !isDirectInventoryUseItem(item.category, item.link)) {
      return false;
    }
    if (!(yield* canUseMemberItem(item.memberOnly))) return false;

    const startingQuantity = item.quantity;
    const sent = yield* bridge
      .invoke("inventory.use", [{ itemId: item.itemId }], Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));
    if (!sent) return false;

    return yield* wait.until(
      get(item.itemId).pipe(
        Effect.map(
          (current) => current === null || current.quantity < startingQuantity,
        ),
      ),
      { timeout: "5 seconds" },
    );
  });

  const equipByEnhancementEffect = Effect.fn("Inventory.equipByEnhancement")(
    function* (selector: EquipEnhancementSelector, options?: EquipOptions) {
      const decoded = decodeEquipEnhancementSelector(selector);
      if (Option.isNone(decoded)) return false;
      const normalized = decoded.value;
      if (
        normalized.slot === undefined &&
        normalized.special === undefined &&
        normalized.enhancement.toLowerCase() === "forge"
      ) {
        return false;
      }

      const items = yield* getAll();
      let target: (typeof items)[number] | undefined;
      for (const item of items) {
        if (item.category.trim().toLowerCase() === "enhancement") continue;
        const resolution = resolveEnhancementStrategy(
          item,
          normalized.enhancement,
          item.enhancement?.level ?? 0,
          normalized.special,
        );
        if (
          !resolution.ok ||
          (normalized.slot !== undefined &&
            resolution.strategy.slot !== normalized.slot) ||
          !matchesAppliedEnhancement(item, resolution.strategy)
        ) {
          continue;
        }
        if (
          target === undefined ||
          (item.enhancement?.level ?? 0) > (target.enhancement?.level ?? 0)
        ) {
          target = item;
        }
      }
      if (target === undefined) return false;
      return yield* equip(target.itemId, options);
    },
  );
  const equipByEnhancement = (
    selector: EquipEnhancementSelector,
    options?: EquipOptions,
  ) => equipByEnhancementEffect(selector, options);

  const getAvailableSlots = (pool: InventoryPool = "bag") =>
    Effect.zipWith(getSlots(pool), getUsedSlots(pool), (slots, used) =>
      Math.max(0, slots - used),
    );

  const unequipConsumable = Effect.fn("Inventory.unequipConsumable")(
    function* (selector: ItemQuery) {
      const item = yield* get(selector);
      if (item === null || item.category !== "Item") return false;

      const sent = yield* bridge
        .invoke(
          "inventory.unequipConsumable",
          [{ itemId: item.itemId }],
          Schema.Boolean,
        )
        .pipe(Effect.map(Option.getOrElse(() => false)));
      if (!sent) return false;

      yield* host.command({ kind: "refreshEquippedConsumable" });
      const current = yield* get(item.itemId);
      return current?.equipped !== true;
    },
    consumableEquipment.withPermit,
    Effect.timeoutOption("5 seconds"),
    Effect.map(Option.getOrElse(() => false)),
  );

  return {
    canAccept,
    contains,
    equip,
    equipByEnhancement,
    equipClaimedConsumable: (itemId: number, claim: string) => equipEffect(itemId, undefined, claim),
    get,
    getAll,
    getAvailableSlots,
    getSlots,
    getUsedSlots,
    unequipConsumable,
    use,
    wear,
  };
};

export type Inventory = ReturnType<typeof makeInventory>;
