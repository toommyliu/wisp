import { normalizeItemQuantity, toItemSelector } from "@lucent/game";
import type { ItemQuery, LiveItem } from "@lucent/game";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { operate } from "../../../host/operations";
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
import type { Inventory } from "./Inventory";
import type { Wait } from "./Wait";

export interface BankOpenOptions {
  /**
   * Whether to reload bank items before opening the bank.
   * @defaultValue false
   */
  readonly force?: boolean;
}

const TransferResponse = Schema.Struct({
  ItemID: PositiveWireInt,
  bSuccess: Schema.optionalKey(WireBoolean),
});
const SwapResponse = Schema.Struct({
  bankItemID: PositiveWireInt,
  invItemID: PositiveWireInt,
});

const decodeTransferResponse = Schema.decodeUnknownOption(TransferResponse);
const decodeSwapResponse = Schema.decodeUnknownOption(SwapResponse);

const houseCanAccept = (itemId: number, house: House) =>
  house
    .get(itemId)
    .pipe(
      Effect.flatMap((current) =>
        current !== null
          ? Effect.succeed(true)
          : house
              .getAvailableSlots()
              .pipe(Effect.map((available) => available > 0)),
      ),
    );

export const makeBank = Effect.fnUntraced(function* (
  bridge: BridgeService,
  host: HostStateService,
  inventory: Inventory,
  house: House,
  wait: Wait,
) {
  const operationHost = globalThis.__host;

  const isOpen = () =>
    bridge
      .invoke("bank.isOpen", undefined, Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));

  const getAll = () =>
    isHydrated(host, "bank").pipe(
      Effect.flatMap((hydrated) =>
        hydrated ? readItems(host, "bank") : Effect.succeed([]),
      ),
    );

  const get = (selector: ItemQuery) =>
    isHydrated(host, "bank").pipe(
      Effect.flatMap((hydrated) =>
        hydrated ? readItem(host, "bank", selector) : Effect.succeed(null),
      ),
    );

  const getSlots = () =>
    bridge
      .invoke("bank.getSlots", undefined, WireInt)
      .pipe(Effect.map(Option.getOrElse(() => 0)));

  const getUsedSlots = () =>
    bridge
      .invoke("bank.getUsedSlots", undefined, WireInt)
      .pipe(Effect.map(Option.getOrElse(() => 0)));

  const load = (force = false) =>
    operate({ kind: "loadBank", force }, operationHost).pipe(
      Effect.map((answer) => answer.kind === "loadBank" && answer.value),
      Effect.catch(() => Effect.succeed(false)),
    );

  const open = (options: BankOpenOptions = {}) =>
    operate({ kind: "openBank", force: options.force ?? false }, operationHost).pipe(
      Effect.map((answer) => answer.kind === "openBank" && answer.value),
      Effect.catch(() => Effect.succeed(false)),
    );

  const contains = (selector: ItemQuery, quantity?: number) =>
    get(selector).pipe(
      Effect.map(
        (item) =>
          item !== null && item.quantity >= normalizeItemQuantity(quantity),
      ),
    );

  const bankCanAccept = (item: LiveItem) =>
    item.coins
      ? Effect.succeed(true)
      : getAvailableSlots().pipe(Effect.map((slots) => slots > 0));

  const deposit = (selector: ItemQuery) => {
    return Effect.gen(function* () {
      if (!(yield* open())) return false;
      const inventoryItem = yield* inventory.get(selector);
      if (
        inventoryItem === null ||
        inventoryItem.category === "Class" ||
        inventoryItem.equipped ||
        !(yield* bankCanAccept(inventoryItem))
      ) {
        return false;
      }
      let response: typeof TransferResponse.Type | undefined;
      const packet = yield* wait.forPacket(
        {
          command: "bankFromInv",
          direction: "extension",
          predicate: (candidate) => {
            const decoded = decodeTransferResponse(packetData(candidate));
            if (
              Option.isNone(decoded) ||
              decoded.value.ItemID !== inventoryItem.itemId
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
            .invoke("bank.deposit", [toItemSelector(selector)], Schema.Boolean)
            .pipe(Effect.map(Option.getOrElse(() => false))),
        },
      );
      if (packet === null || response?.bSuccess === false) return false;

      const [projectedInventoryItem, projectedBankItem] = yield* Effect.all([
        readItem(host, "inventory", inventoryItem.itemId),
        readItem(host, "bank", inventoryItem.itemId),
      ]);
      return projectedInventoryItem === null && projectedBankItem !== null;
    });
  };

  const withdraw = (selector: ItemQuery) => {
    return Effect.gen(function* () {
      if (!(yield* load())) return false;
      const bankItem = yield* get(selector);
      if (bankItem === null) return false;
      if (!(yield* open())) return false;
      const canAccept = bankItem.houseItem
        ? houseCanAccept(bankItem.itemId, house)
        : inventory.canAccept(bankItem);
      if (!(yield* canAccept)) {
        return false;
      }
      let response: typeof TransferResponse.Type | undefined;
      const packet = yield* wait.forPacket(
        {
          command: "bankToInv",
          direction: "extension",
          predicate: (candidate) => {
            const decoded = decodeTransferResponse(packetData(candidate));
            if (
              Option.isNone(decoded) ||
              decoded.value.ItemID !== bankItem.itemId
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
            .invoke("bank.withdraw", [toItemSelector(selector)], Schema.Boolean)
            .pipe(Effect.map(Option.getOrElse(() => false))),
        },
      );
      if (packet === null || response?.bSuccess === false) return false;

      const [bank, inventoryItem, houseItem] = yield* Effect.all([
        readItem(host, "bank", bankItem.itemId),
        readItem(host, "inventory", bankItem.itemId),
        readItem(host, "house", bankItem.itemId),
      ]);
      return bank === null && (inventoryItem !== null || houseItem !== null);
    });
  };

  const swap = (inventorySelector: ItemQuery, bankSelector: ItemQuery) => {
    return Effect.gen(function* () {
      if (!(yield* open())) return false;
      const inventoryItem = yield* inventory.get(inventorySelector);
      const bankItem = yield* get(bankSelector);
      if (
        inventoryItem === null ||
        bankItem === null ||
        inventoryItem.category === "Class" ||
        inventoryItem.equipped
      ) {
        return false;
      }
      if (
        !inventoryItem.coins &&
        bankItem.coins &&
        !(yield* bankCanAccept(inventoryItem))
      ) {
        return false;
      }
      if (!(yield* inventory.canAccept(bankItem, inventoryItem))) return false;
      const packet = yield* wait.forPacket(
        {
          command: "bankSwapInv",
          direction: "extension",
          predicate: (candidate) => {
            const decoded = decodeSwapResponse(packetData(candidate));
            return (
              Option.isSome(decoded) &&
              decoded.value.invItemID === inventoryItem.itemId &&
              decoded.value.bankItemID === bankItem.itemId
            );
          },
          encoding: "json",
        },
        {
          timeout: "5 seconds",
          trigger: bridge
            .invoke(
              "bank.swap",
              [
                toItemSelector(inventoryItem.itemId),
                toItemSelector(bankItem.itemId),
              ],
              Schema.Boolean,
            )
            .pipe(Effect.map(Option.getOrElse(() => false))),
        },
      );
      if (packet === null) return false;

      const [newBankItem, newInventoryItem, newHouseItem] = yield* Effect.all([
        readItem(host, "bank", inventoryItem.itemId),
        readItem(host, "inventory", bankItem.itemId),
        readItem(host, "house", bankItem.itemId),
      ]);
      return (
        newBankItem !== null &&
        (newInventoryItem !== null || newHouseItem !== null)
      );
    });
  };

  const depositBatch = (selectors: readonly ItemQuery[]) =>
    Effect.forEach(selectors, deposit, { concurrency: 1 });

  const withdrawBatch = (selectors: readonly ItemQuery[]) =>
    Effect.forEach(selectors, withdraw, { concurrency: 1 });

  const getAvailableSlots = () =>
    Effect.zipWith(getSlots(), getUsedSlots(), (slots, used) =>
      Math.max(0, slots - used),
    );

  return {
    contains,
    deposit,
    depositBatch,
    get,
    getAll,
    getAvailableSlots,
    getSlots,
    getUsedSlots,
    isOpen,
    load,
    open,
    swap,
    withdraw,
    withdrawBatch,
  };
});

export type Bank = Effect.Success<ReturnType<typeof makeBank>>;
