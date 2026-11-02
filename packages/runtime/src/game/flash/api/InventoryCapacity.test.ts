import { describe, expect, it } from "@effect/vitest";
import type { ItemSnapshot } from "@lucent/game";
import * as Effect from "effect/Effect";
import { vi } from "vitest";

import {
  emitExtension,
  item,
  makeFakeApi,
  makeFakeBankOperations,
  makeFakeHostState,
  moveItem,
} from "../testing/FakeHost";

const owned = (
  itemId: number,
  name: string,
  category: string,
  extra: {
    readonly equipped?: boolean;
    readonly houseItem?: boolean;
    readonly meta?: string;
  } = {},
): ItemSnapshot => item("inventory", { category, itemId, name, ...extra });
const banked = (
  itemId: number,
  name: string,
  category: string,
  extra: { readonly houseItem?: boolean } = {},
): ItemSnapshot => item("bank", { category, itemId, name, ...extra });

const equipment = owned(1, "Sword", "Sword");
const misc = owned(2, "Resource", "Resource");
const classItem = owned(3, "Class", "Class");
const potion = owned(4, "Potion", "Item", { meta: " 400 " });
const bankEquipment = banked(10, "Bank Sword", "Sword");
const bankMisc = banked(11, "Bank Resource", "Resource");

const makeHarness = Effect.fnUntraced(function* (
  items: readonly ItemSnapshot[],
  bankItems: readonly ItemSnapshot[] = [bankEquipment, bankMisc],
  bankUsedSlots = 2,
  houseItems: readonly ItemSnapshot[] = [],
) {
  const target = {} as Window;
  const state = makeFakeHostState();
  const loadInventory = () => {
    state.items.inventory = [...items];
    state.items.house = houseItems.map((entry) =>
      item("house", { ...entry, context: "house" }),
    );
    state.hydration.inventory += 1;
    state.hydration.house += 1;
  };
  loadInventory();
  const calls = { deposits: 0, swaps: 0, withdrawals: 0 };
  let loaded = false;
  let opened = false;
  let miscSlots = 2;
  const emit = (dataObj: object) => emitExtension(target, dataObj);
  const withdrawTo = (itemId: number) =>
    state.items.bank.find((entry) => entry.itemId === itemId)?.houseItem
      ? "house"
      : "inventory";
  target.swf = {
    "auth.isLoggedIn": () => true,
    "bank.getItems": () => bankItems,
    "bank.getSlots": () => 10,
    "bank.getUsedSlots": () => bankUsedSlots,
    "bank.isLoaded": () => loaded,
    "bank.loadItems": () => {
      loaded = true;
    },
    "bank.isOpen": () => opened,
    "bank.open": () => {
      opened = true;
    },
    "bank.withdraw": (selector: { itemId: number }) => {
      calls.withdrawals++;
      moveItem(state, "bank", withdrawTo(selector.itemId), selector.itemId);
      emit({ cmd: "bankToInv", ItemID: selector.itemId, bSuccess: 1 });
      return true;
    },
    "bank.deposit": (selector: { itemId: number }) => {
      calls.deposits++;
      moveItem(state, "inventory", "bank", selector.itemId);
      emit({ cmd: "bankFromInv", ItemID: selector.itemId, bSuccess: 1 });
      return true;
    },
    "bank.swap": (
      inventorySelector: { itemId: number },
      bankSelector: { itemId: number },
    ) => {
      calls.swaps++;
      const destination = withdrawTo(bankSelector.itemId);
      moveItem(state, "inventory", "bank", inventorySelector.itemId);
      moveItem(state, "bank", destination, bankSelector.itemId);
      emit({
        cmd: "bankSwapInv",
        invItemID: inventorySelector.itemId,
        bankItemID: bankSelector.itemId,
      });
      return true;
    },
    "inventory.getSlots": () => 1,
    "inventory.getMiscSlots": () => miscSlots,
    "house.getSlots": () => 1,
  } as unknown as Window["swf"];
  vi.stubGlobal("__host", makeFakeBankOperations(target, state));
  const api = yield* makeFakeApi(target, state);
  return {
    api,
    calls,
    emitConnection: (status: string) => {
      state.items.inventory = [];
      state.items.house = [];
      state.hydration.inventory = 0;
      state.hydration.house = 0;
      target.onConnection?.(status);
    },
    reloadInventory: () => {
      loadInventory();
      emit({ cmd: "loadInventoryBig", items: [], hitems: [] });
    },
    setMiscSlots: (slots: number) => {
      miscSlots = slots;
    },
  };
});

describe("inventory capacity after the AQW inventory update", () => {
  it.effect(
    "counts equipment slots separately from misc items and classes",
    () =>
      Effect.gen(function* () {
        const { api } = yield* makeHarness([
          misc,
          classItem,
          owned(5, "Note", "Note"),
          owned(6, "Quest Item", "Quest Item"),
          owned(7, "Misc Item", "Item"),
          owned(8, "Guild Item", "Guild"),
        ]);
        expect(yield* api.inventory.getUsedSlots()).toBe(0);
        expect(yield* api.inventory.getAvailableSlots()).toBe(1);
        expect(yield* api.bank.withdraw(10)).toBe(true);
        expect(yield* api.inventory.contains(10)).toBe(true);
        expect(yield* api.bank.contains(10)).toBe(false);
        expect(yield* api.inventory.getAvailableSlots()).toBe(0);
      }),
  );

  it.effect("counts action-slot consumables as equipment", () =>
    Effect.gen(function* () {
      const { api, calls } = yield* makeHarness([potion]);
      expect(yield* api.inventory.getUsedSlots()).toBe(1);
      expect(yield* api.bank.withdraw(10)).toBe(false);
      expect(calls.withdrawals).toBe(0);
    }),
  );

  it.effect(
    "withdraws misc items into their own pool when equipment is full",
    () =>
      Effect.gen(function* () {
        const { api } = yield* makeHarness([equipment, misc]);
        expect(yield* api.bank.withdraw(11)).toBe(true);
        expect(yield* api.inventory.contains(11)).toBe(true);
        expect(yield* api.inventory.getUsedSlots()).toBe(1);
      }),
  );

  it.effect("rejects misc withdrawals when the misc pool is full", () =>
    Effect.gen(function* () {
      const { api, calls } = yield* makeHarness([
        misc,
        owned(5, "Quest Item", "Quest Item"),
      ]);
      expect(yield* api.bank.withdraw(11)).toBe(false);
      expect(calls.withdrawals).toBe(0);
      expect(yield* api.bank.contains(11)).toBe(true);
    }),
  );

  it.effect(
    "allows a misc swap to free a misc slot but not an equipment slot",
    () =>
      Effect.gen(function* () {
        const { api, calls } = yield* makeHarness([
          equipment,
          misc,
          owned(5, "Quest Item", "Quest Item"),
        ]);
        expect(yield* api.bank.swap(1, 11)).toBe(false);
        expect(calls.swaps).toBe(0);
        expect(yield* api.bank.swap(2, 11)).toBe(true);
        expect(yield* api.inventory.contains(11)).toBe(true);
        expect(yield* api.bank.contains(2)).toBe(true);
      }),
  );

  it.effect(
    "allows swaps only when they leave room in the destination pool",
    () =>
      Effect.gen(function* () {
        const { api, calls } = yield* makeHarness([equipment, misc]);
        expect(yield* api.bank.swap(2, 10)).toBe(false);
        expect(calls.swaps).toBe(0);
        expect(yield* api.bank.swap(1, 10)).toBe(true);
        expect(calls.swaps).toBe(1);
        expect(yield* api.inventory.contains(10)).toBe(true);
        expect(yield* api.bank.contains(1)).toBe(true);
      }),
  );

  it.effect("allows swaps within an already overfilled pool", () =>
    Effect.gen(function* () {
      const { api } = yield* makeHarness([
        misc,
        owned(5, "Quest Item", "Quest Item"),
        owned(6, "Note", "Note"),
      ]);
      expect(yield* api.bank.withdraw(11)).toBe(false);
      expect(yield* api.bank.swap(2, 11)).toBe(true);
      expect(yield* api.inventory.contains(11)).toBe(true);
      expect(yield* api.bank.contains(2)).toBe(true);
    }),
  );

  it.effect("rejects banking classes and equipped items", () =>
    Effect.gen(function* () {
      const { api, calls } = yield* makeHarness([
        classItem,
        owned(1, "Sword", "Sword", { equipped: true }),
      ]);
      expect(yield* api.bank.deposit(3)).toBe(false);
      expect(yield* api.bank.deposit(1)).toBe(false);
      expect(yield* api.bank.swap(3, 10)).toBe(false);
      expect(yield* api.bank.swap(1, 10)).toBe(false);
      expect(calls).toEqual({ deposits: 0, swaps: 0, withdrawals: 0 });
    }),
  );

  it.effect("uses the client's bank count to reject a full bank", () =>
    Effect.gen(function* () {
      const full = yield* makeHarness(
        [equipment],
        [bankEquipment, bankMisc],
        10,
      );
      expect(yield* full.api.bank.deposit(1)).toBe(false);
      expect(full.calls.deposits).toBe(0);
      expect(yield* full.api.inventory.contains(1)).toBe(true);

      const available = yield* makeHarness([equipment]);
      expect(yield* available.api.bank.deposit(1)).toBe(true);
      expect(yield* available.api.bank.contains(1)).toBe(true);
      expect(yield* available.api.inventory.contains(1)).toBe(false);
    }),
  );

  it.effect(
    "withdraws Guild items into inventory without using a bag slot",
    () =>
      Effect.gen(function* () {
        const { api } = yield* makeHarness(
          [equipment],
          [banked(12, "Guild Item", "Guild")],
        );
        expect(yield* api.bank.withdraw(12)).toBe(true);
        expect(yield* api.inventory.contains(12)).toBe(true);
        expect(yield* api.house.contains(12)).toBe(false);
        expect(yield* api.inventory.getUsedSlots()).toBe(1);
      }),
  );

  it.effect(
    "allows native house and Guild swaps when house storage is full",
    () =>
      Effect.gen(function* () {
        for (const bankItem of [
          banked(12, "Guild Item", "Guild"),
          banked(12, "House Item", "Floor Item", { houseItem: true }),
        ]) {
          const { api, calls } = yield* makeHarness(
            [equipment],
            [bankItem],
            1,
            [owned(20, "Owned House Item", "", { houseItem: true })],
          );
          expect(yield* api.bank.withdraw(12)).toBe(false);
          expect(calls.withdrawals).toBe(0);
          expect(yield* api.bank.swap(1, 12)).toBe(true);
          expect(calls.swaps).toBe(1);
          expect(yield* api.bank.contains(1)).toBe(true);
          const destination = bankItem.houseItem ? api.house : api.inventory;
          expect(yield* destination.contains(12)).toBe(true);
        }
      }),
  );

  it.effect("reports bag and misc capacity independently", () =>
    Effect.gen(function* () {
      const { api } = yield* makeHarness([
        equipment,
        misc,
        classItem,
        owned(8, "Guild Item", "Guild"),
        potion,
      ]);
      expect(yield* api.inventory.getSlots()).toBe(1);
      expect(yield* api.inventory.getSlots("bag")).toBe(1);
      expect(yield* api.inventory.getUsedSlots()).toBe(2);
      expect(yield* api.inventory.getAvailableSlots()).toBe(0);
      expect(yield* api.inventory.getSlots("misc")).toBe(2);
      expect(yield* api.inventory.getUsedSlots("misc")).toBe(1);
      expect(yield* api.inventory.getAvailableSlots("misc")).toBe(1);
    }),
  );

  it.effect(
    "rejects a new misc withdrawal when availability reaches zero",
    () =>
      Effect.gen(function* () {
        const { api, calls } = yield* makeHarness(
          [misc],
          [bankMisc, banked(12, "Another Resource", "Resource")],
        );
        expect(yield* api.bank.withdraw(11)).toBe(true);
        expect(yield* api.inventory.getUsedSlots("misc")).toBe(2);
        expect(yield* api.inventory.getAvailableSlots("misc")).toBe(0);
        expect(yield* api.inventory.contains(11)).toBe(true);
        expect(yield* api.bank.contains(11)).toBe(false);
        expect(calls.withdrawals).toBe(1);
        expect(yield* api.bank.withdraw(12)).toBe(false);
        expect(calls.withdrawals).toBe(1);
        expect(yield* api.inventory.contains(12)).toBe(false);
        expect(yield* api.bank.contains(12)).toBe(true);
      }),
  );

  it.effect("uses the current misc limit for reads and withdrawals", () =>
    Effect.gen(function* () {
      const { api, setMiscSlots } = yield* makeHarness([
        misc,
        owned(5, "Quest Item", "Quest Item"),
      ]);
      expect(yield* api.inventory.getSlots("misc")).toBe(2);
      expect(yield* api.inventory.getAvailableSlots("misc")).toBe(0);
      setMiscSlots(3);
      expect(yield* api.inventory.getSlots("misc")).toBe(3);
      expect(yield* api.inventory.getAvailableSlots("misc")).toBe(1);
      expect(yield* api.bank.withdraw(11)).toBe(true);
      expect(yield* api.inventory.getUsedSlots("misc")).toBe(3);
      expect(yield* api.inventory.getAvailableSlots("misc")).toBe(0);
    }),
  );

  it.effect(
    "resets capacity until inventory reloads after connection events",
    () =>
      Effect.gen(function* () {
        const { api, emitConnection, reloadInventory } = yield* makeHarness([
          equipment,
          misc,
        ]);
        expect(yield* api.inventory.getSlots("misc")).toBe(2);
        expect(yield* api.inventory.getAvailableSlots("misc")).toBe(1);
        expect(yield* api.inventory.getSlots()).toBe(1);

        for (const status of [
          "OnConnection",
          "OnConnectionLost",
          "OnConnectionFailed",
        ]) {
          const reset = yield* api.wait.forEvent(
            { type: "connection", status },
            {
              trigger: Effect.sync(() => {
                emitConnection(status);
                return true;
              }),
            },
          );
          expect(reset).toEqual({ type: "connection", status });
          expect(yield* api.inventory.getSlots("misc")).toBe(0);
          expect(yield* api.inventory.getAvailableSlots("misc")).toBe(0);
          expect(yield* api.inventory.getSlots()).toBe(0);

          const reloadedInventory = yield* api.wait.forPacket(
            {
              command: "loadInventoryBig",
              direction: "extension",
              encoding: "json",
            },
            {
              trigger: Effect.sync(() => {
                reloadInventory();
                return true;
              }),
            },
          );
          expect(reloadedInventory?.command).toBe("loadInventoryBig");
          expect(yield* api.inventory.getSlots("misc")).toBe(2);
          expect(yield* api.inventory.getUsedSlots("misc")).toBe(1);
          expect(yield* api.inventory.getAvailableSlots("misc")).toBe(1);
          expect(yield* api.inventory.getSlots()).toBe(1);
          expect(yield* api.inventory.getUsedSlots()).toBe(1);
          expect(yield* api.inventory.getAvailableSlots()).toBe(0);
        }
      }),
  );
});
