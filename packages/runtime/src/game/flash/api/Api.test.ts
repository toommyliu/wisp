import { describe, expect, it } from "@effect/vitest";
import { EntityState, type ItemSnapshot } from "@lucent/game";
import * as Effect from "effect/Effect";
import { vi } from "vitest";

import {
  editItem,
  emitExtension,
  item,
  makeFakeApi,
  makeFakeBankOperations,
  makeFakeHostState,
  monster,
  moveItem,
  player,
  type FakeHostState,
} from "../testing/FakeHost";

const loadedBank: readonly ItemSnapshot[] = [
  item("bank", { coins: true, itemId: 42, name: "Indexed Item", quantity: 2 }),
  item("bank", { houseItem: true, itemId: 43, name: "Banked House Item" }),
  item("bank", { itemId: 44, name: "Occupied Slot" }),
  item("bank", {
    category: "Class",
    equipmentSlot: "ar",
    itemId: 45,
    name: "Banked Class",
    quantity: 302_500,
  }),
];

const testInventory: readonly ItemSnapshot[] = [
  item("inventory", { itemId: 50, name: "Normal Item" }),
  item("inventory", { coins: true, itemId: 51, name: "Coin Item" }),
  item("inventory", {
    equipmentSlot: "Weapon",
    itemId: 52,
    name: "Wearable Item",
    wearable: true,
    worn: false,
  }),
  item("inventory", {
    equipmentSlot: "ba",
    itemId: 53,
    memberOnly: true,
    name: "Member Item",
    wearable: true,
    worn: false,
  }),
  item("inventory", {
    equipmentSlot: "Weapon",
    itemId: 54,
    name: "Cosmetic Item",
    wearable: true,
    worn: false,
  }),
];

const inSlot = (state: FakeHostState, itemId: number) =>
  state.items.inventory.find((entry) => entry.itemId === itemId)?.equipmentSlot;

const markInSlot = (
  state: FakeHostState,
  itemId: number,
  field: "equipped" | "worn",
) => {
  const slot = inSlot(state, itemId);
  state.items.inventory = state.items.inventory.map((entry) =>
    entry.equipmentSlot === slot
      ? editItem(entry, { [field]: entry.itemId === itemId })
      : entry,
  );
};

const makeTarget = (state: FakeHostState) => {
  const calls = {
    actions: [] as string[],
    bankOpens: 0,
    bankLoadForces: [] as boolean[],
    deposits: 0,
    equips: 0,
    hairShopLoads: 0,
    shopLoads: 0,
    skillUses: [] as {
      force: boolean;
      index: string;
      selector: { monsterMapId: number } | { name: string } | null;
    }[],
    swaps: 0,
    wears: 0,
    withdrawals: 0,
  };
  let failBankLoad = false;
  let bankLoaded = false;
  let bankOpen = false;
  let bankItems: readonly ItemSnapshot[] = [];
  let cachedShopId = 0;
  let openShopId = 0;
  const target = {} as Window;

  target.swf = {
    "auth.isLoggedIn": () => true,
    "bank.deposit": () => {
      calls.deposits += 1;
      moveItem(state, "inventory", "bank", 51);
      emitExtension(target, {
        ItemID: 51,
        bSuccess: 1,
        cmd: "bankFromInv",
      });
      return true;
    },
    "bank.getSlots": () => 2,
    "bank.getUsedSlots": () => 2,
    "bank.getItems": () => bankItems,
    "bank.isLoaded": () => bankLoaded,
    "bank.loadItems": (force = false) => {
      calls.bankLoadForces.push(force);
      if (failBankLoad) throw new Error("load failed");
      bankItems = loadedBank;
      bankLoaded = true;
    },
    "bank.isOpen": () => bankOpen,
    "bank.open": () => {
      calls.bankOpens += 1;
      bankOpen = true;
    },
    "bank.swap": (
      inventorySelector: { itemId: number },
      bankSelector: { itemId: number },
    ) => {
      calls.swaps += 1;
      moveItem(state, "inventory", "bank", inventorySelector.itemId);
      moveItem(state, "bank", "inventory", bankSelector.itemId);
      emitExtension(target, {
        bankItemID: bankSelector.itemId,
        cmd: "bankSwapInv",
        invItemID: inventorySelector.itemId,
      });
      return true;
    },
    "bank.withdraw": () => {
      calls.withdrawals += 1;
      return false;
    },
    "combat.getSkillCooldownRemaining": () => 0,
    "combat.useSkill": (
      index: string,
      selector: { monsterMapId: number } | { name: string } | null = null,
      force = false,
    ) => {
      calls.skillUses.push({ force, index, selector });
      return true;
    },
    "house.getSlots": () => 1,
    "inventory.equip": (selector: { itemId: number }) => {
      calls.equips += 1;
      markInSlot(state, selector.itemId, "equipped");
      emitExtension(target, {
        ItemID: selector.itemId,
        cmd: "equipItem",
        strES: "Weapon",
        uid: 1,
      });
      return true;
    },
    "inventory.wear": (selector: { itemId: number }) => {
      calls.wears += 1;
      markInSlot(state, selector.itemId, "worn");
      emitExtension(target, {
        ItemID: selector.itemId,
        cmd: "wearItem",
        sES: "Weapon",
        success: 1,
        uid: 2,
      });
      emitExtension(target, {
        ItemID: selector.itemId,
        cmd: "wearItem",
        sES: "Weapon",
        success: 1,
        uid: 1,
      });
      return true;
    },
    "inventory.getSlots": () => 4,
    "player.getClassRank": () => {
      throw new Error("Class rank must not use the bridge");
    },
    "player.getUserId": () => 1,
    "player.isMember": () => false,
    "shops.isOpen": (shopId = 0) =>
      openShopId !== 0 && (shopId === 0 || shopId === openShopId),
    "shops.canBuyItem": () => true,
    "shops.buy": () => {
      bankItems = loadedBank.map((entry) =>
        entry.itemId === 42 ? editItem(entry, { quantity: 3 }) : entry,
      );
      emitExtension(target, {
        cmd: "buyItem",
        bitSuccess: 1,
        bBank: 1,
        CharItemID: 420,
        ItemID: 42,
        iQty: 1,
      });
    },
    "shops.load": (shopId: number) => {
      calls.shopLoads += 1;
      openShopId = shopId;
      if (cachedShopId === shopId) return;
      cachedShopId = shopId;
      state.shop = {
        house: false,
        id: shopId,
        items: [],
        limited: false,
        merge: false,
        name: "Action-locked Shop",
      };
      emitExtension(target, {
        ShopID: shopId,
        cmd: "loadShop",
        items: [],
        sName: "Action-locked Shop",
      });
    },
    "shops.loadHairShop": () => {
      calls.hairShopLoads += 1;
    },
    "world.isActionAvailable": (action: string) => {
      calls.actions.push(action);
      return true;
    },
    "world.getAvailableMonsterMapIds": () => [2],
  } as unknown as Window["swf"];

  return {
    calls,
    closeBankUi: () => {
      bankOpen = false;
    },
    closeShopUi: () => {
      openShopId = 0;
    },
    failNextBankLoad: () => {
      failBankLoad = true;
    },
    resetBankSession: () => {
      bankItems = [];
      bankLoaded = false;
      bankOpen = false;
      failBankLoad = false;
    },
    target,
  };
};

const makeApiHarness = () =>
  Effect.gen(function* () {
    const state = makeFakeHostState();
    const harness = makeTarget(state);
    vi.stubGlobal("__host", makeFakeBankOperations(harness.target, state));
    const api = yield* makeFakeApi(harness.target, state);
    return { ...harness, api, state };
  });

const loadTestInventory = (state: FakeHostState) => {
  state.items.inventory = [...testInventory];
  state.items.house = [
    item("house", { houseItem: true, itemId: 60, name: "Placed House Item" }),
  ];
  state.hydration.inventory += 1;
  state.hydration.house += 1;
};

describe("Api", () => {
  it.effect("resolves named skill targets from available monsters", () =>
    Effect.gen(function* () {
      const { api, calls, state } = yield* makeApiHarness();
      state.map = { id: 1, name: "test", roomNumber: 1 };
      state.players = [player({ entityId: 1, username: "Hero" })];
      state.selfEntityId = 1;
      state.monsters = [
        monster({
          hp: 0,
          monsterMapId: 1,
          name: "Slime",
          state: EntityState.Dead,
        }),
        monster({
          monsterMapId: 2,
          name: "Slime",
          state: EntityState.Idle,
        }),
      ];

      expect(
        yield* api.combat.useSkill(1, {
          force: true,
          target: "Slime",
        }),
      ).toBe(true);
      expect(calls.skillUses).toEqual([
        { force: true, index: "1", selector: { monsterMapId: 2 } },
      ]);
    }),
  );

  it.effect("reads class ranks from projections without bridge calls", () =>
    Effect.gen(function* () {
      const { api, calls, state } = yield* makeApiHarness();

      expect(yield* api.player.getClassRank()).toBeNull();
      expect(yield* api.player.getClassRank(45)).toBeNull();
      expect(calls.bankLoadForces).toEqual([]);

      expect(yield* api.bank.open()).toBe(true);
      expect(yield* api.bank.get(45)).toBeNull();
      expect(yield* api.player.getClassRank(45)).toBeNull();

      state.items.inventory = [
        item("inventory", {
          category: "Class",
          equipmentSlot: "ar",
          itemId: 45,
          name: "Owned Class",
          quantity: 302_500,
        }),
        item("inventory", {
          category: "Class",
          equipmentSlot: "ar",
          equipped: true,
          itemId: 46,
          name: "Equipped Class",
          quantity: 10_000,
        }),
      ];
      state.hydration.inventory += 1;
      expect(yield* api.player.getClassRank()).toBe(4);
      expect(yield* api.player.getClassRank(46)).toBe(4);
      expect(yield* api.player.getClassRank(45)).toBe(10);
    }),
  );

  it.effect("reuses the shared bank for house withdrawals", () =>
    Effect.gen(function* () {
      const { api, calls } = yield* makeApiHarness();

      expect(yield* api.bank.isOpen()).toBe(false);
      expect(yield* api.bank.open()).toBe(true);
      expect(yield* api.bank.isOpen()).toBe(true);
      expect(calls.bankOpens).toBe(1);

      expect(yield* api.bank.open()).toBe(true);
      expect(calls.bankOpens).toBe(1);

      expect(yield* api.bank.open({ force: true })).toBe(true);
      expect(calls.bankLoadForces).toEqual([true, true]);
      expect(calls.bankOpens).toBe(1);

      expect(yield* api.bank.withdraw(43)).toBe(false);
      expect(calls.withdrawals).toBe(1);
      expect(calls.bankOpens).toBe(1);
      expect((yield* api.bank.get(43))?.context).toBe("bank");
      expect(yield* api.house.get(43)).toBeNull();
    }),
  );

  it.effect(
    "caches bank loads until disconnect and retains items after a failed reload",
    () =>
      Effect.gen(function* () {
        const {
          api,
          calls,
          resetBankSession,
          state,
          target,
          failNextBankLoad,
        } = yield* makeApiHarness();
        expect(yield* api.bank.getAll()).toEqual([]);
        expect(yield* api.bank.open()).toBe(true);
        expect(calls.bankLoadForces).toEqual([true]);
        expect((yield* api.bank.getAll())[0]?.quantity).toBe(2);
        expect(calls.bankOpens).toBe(1);

        expect(yield* api.bank.open()).toBe(true);
        expect(calls.bankLoadForces).toEqual([true]);
        expect(calls.bankOpens).toBe(1);

        const disconnected = yield* api.wait.forEvent(
          { type: "connection" },
          {
            trigger: Effect.sync(() => {
              resetBankSession();
              state.items.bank = [];
              state.hydration.bank = 0;
              target.onConnection?.("OnConnectionLost");
              return true;
            }),
          },
        );
        expect(disconnected).not.toBeNull();
        expect(yield* api.bank.getAll()).toEqual([]);

        expect(yield* api.bank.open()).toBe(true);
        expect(calls.bankLoadForces).toEqual([true, true]);
        expect(calls.bankOpens).toBe(2);
        expect((yield* api.bank.getAll())[0]?.itemId).toBe(42);

        failNextBankLoad();
        expect(yield* api.bank.load(true)).toBe(false);
        expect(calls.bankLoadForces).toEqual([true, true, true]);
        const retained = yield* api.bank.getAll();
        expect(retained[0]?.itemId).toBe(42);
      }),
  );

  it.effect("refreshes the bank after a purchase into it", () =>
    Effect.gen(function* () {
      const { api, state } = yield* makeApiHarness();
      expect(yield* api.bank.load()).toBe(true);
      state.shop = {
        house: false,
        id: 1,
        items: [item("shop", { itemId: 42, name: "Indexed Item" })],
        limited: false,
        merge: false,
        name: "Shop",
      };
      state.items.shop = [item("shop", { itemId: 42, name: "Indexed Item" })];
      expect(yield* api.shops.buy(42)).toBe(true);
      expect((yield* api.bank.get(42))?.quantity).toBe(3);
    }),
  );

  it.effect("guards full containers and verifies deposits and swaps", () =>
    Effect.gen(function* () {
      const { api, calls, closeBankUi, state } = yield* makeApiHarness();
      yield* api.bank.open();
      loadTestInventory(state);
      expect(yield* api.house.contains(60)).toBe(true);
      expect(yield* api.house.contains(60, 2)).toBe(false);
      expect(yield* api.bank.getAvailableSlots()).toBe(0);
      expect(yield* api.bank.deposit(50)).toBe(false);
      expect(yield* api.bank.withdraw(42)).toBe(false);
      expect(yield* api.bank.withdraw(43)).toBe(false);
      expect(calls.deposits).toBe(0);
      expect(calls.withdrawals).toBe(0);

      const opensBeforeDeposit = calls.bankOpens;
      closeBankUi();
      expect(yield* api.bank.deposit(51)).toBe(true);
      expect(calls.deposits).toBe(1);
      expect(calls.bankOpens).toBe(opensBeforeDeposit + 1);
      expect(calls.bankLoadForces).toEqual([true]);
      expect(yield* api.bank.contains(51)).toBe(true);
      expect(yield* api.inventory.contains(51)).toBe(false);

      expect(yield* api.bank.swap(50, 42)).toBe(false);
      expect(calls.swaps).toBe(0);
      expect(yield* api.bank.swap(50, 44)).toBe(true);
      expect(calls.swaps).toBe(1);
      expect(yield* api.bank.contains(50)).toBe(true);
      expect(yield* api.inventory.contains(44)).toBe(true);
    }),
  );

  it.effect(
    "keeps cosmetic wear distinct from equipment and rejects member-only items",
    () =>
      Effect.gen(function* () {
        const { api, calls, state } = yield* makeApiHarness();
        loadTestInventory(state);
        state.players = [player({ entityId: 1, username: "Hero" })];
        state.selfEntityId = 1;
        expect(yield* api.inventory.wear(54)).toBe(true);
        expect(calls.wears).toBe(1);
        const cosmetic = yield* api.inventory.get(54);
        expect(cosmetic).not.toBeNull();
        expect(cosmetic?.worn).toBe(true);
        expect(cosmetic?.equipped).toBe(false);

        expect(yield* api.inventory.equip(52, { wear: false })).toBe(true);
        expect(calls.equips).toBe(1);
        expect(calls.wears).toBe(1);
        expect((yield* api.inventory.get(52))?.equipped).toBe(true);
        expect((yield* api.inventory.get(52))?.worn).toBe(false);
        expect((yield* api.inventory.get(54))?.worn).toBe(true);

        expect(yield* api.inventory.equip(52)).toBe(true);
        expect(calls.equips).toBe(1);
        expect(calls.wears).toBe(2);
        expect((yield* api.inventory.get(52))?.worn).toBe(true);
        expect((yield* api.inventory.get(54))?.worn).toBe(false);

        expect(yield* api.inventory.wear(52)).toBe(true);
        expect(yield* api.inventory.wear(50)).toBe(false);
        expect(yield* api.inventory.wear(53)).toBe(false);
        expect(calls.wears).toBe(2);

        expect(yield* api.inventory.equip(53)).toBe(false);
        expect(calls.equips).toBe(1);

        expect(calls.actions).toContain("wearItem");
      }),
  );

  it.effect("reopens cached shops and dispatches hair-shop actions", () =>
    Effect.gen(function* () {
      const { api, calls, closeShopUi } = yield* makeApiHarness();
      expect(yield* api.shops.load(101)).toBe(true);
      closeShopUi();
      expect(yield* api.shops.isOpen(101)).toBe(false);
      expect(yield* api.shops.load(101)).toBe(true);
      expect(calls.shopLoads).toBe(2);
      yield* api.shops.openHairShop(202);
      expect(calls.actions).toContain("loadShop");
      expect(calls.actions).toContain("loadHairShop");
      expect(calls.hairShopLoads).toBe(1);
    }),
  );
});
