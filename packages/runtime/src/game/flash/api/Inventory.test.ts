import { describe, expect, it } from "@effect/vitest";
import { afterEach, vi } from "vitest";
import type { ItemSnapshot } from "@lucent/game";
import * as Effect from "effect/Effect";
import * as PubSub from "effect/PubSub";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { completeOperation } from "../../../host/operations";

import type { EquipEnhancementSelector } from "../../EnhancementSelectors";
import type { BridgeService } from "../bridge/Bridge";
import type { Event } from "../contract/Event";
import { makeWait } from "../protocol/Wait";
import type { Packet } from "../contract/Packet";
import {
  editItem,
  item,
  makeFakeHost,
  makeFakeHostState,
} from "../testing/FakeHost";
import { makeHouse } from "./House";
import { makeInventory } from "./Inventory";
import type { Wait } from "./Wait";

afterEach(() => vi.unstubAllGlobals());

const equipment = ({
  enhancementLevel,
  enhancementPatternId,
  enhancementProcId,
  equipmentSlot,
  equipped = false,
  itemId,
}: {
  readonly enhancementLevel: number;
  readonly enhancementPatternId: number;
  readonly enhancementProcId?: number;
  readonly equipmentSlot: string;
  readonly equipped?: boolean;
  readonly itemId: number;
}) =>
  item("inventory", {
    category: equipmentSlot === "ar" ? "Class" : "Equipment",
    enhancement: {
      level: enhancementLevel,
      patternId: enhancementPatternId,
      ...(enhancementProcId === undefined ? {} : { procId: enhancementProcId }),
    },
    equipped,
    equipmentSlot,
    itemId,
    name: `Item ${itemId}`,
  });

const consumable = (
  itemId: number,
  link: string,
  equipped = false,
  category = "Item",
  actionId = 400,
): ItemSnapshot =>
  item("inventory", {
    category,
    description: "Consumable",
    equipped,
    file: "consumable.swf",
    itemId,
    link,
    meta: String(actionId),
    name: `Consumable ${itemId}`,
    quantity: 2,
  });

const makeHarness = (items: readonly ItemSnapshot[]) =>
  Effect.gen(function* () {
    const state = makeFakeHostState();
    state.items.inventory = [...items];
    state.hydration.inventory = 1;

    const packets = yield* PubSub.unbounded<Packet>();
    const events = yield* PubSub.unbounded<Event>();
    const confirmEquip = (data: unknown) =>
      PubSub.publish(packets, {
        command: "seia",
        direction: "extension",
        encoding: "json",
        raw: "",
        data,
      });
    let slotItemId: number | undefined;
    const equippedItemIds: number[] = [];
    const unequippedItemIds: number[] = [];
    const usedItemIds: number[] = [];
    const bridge = {
      invoke: (method: string, args: readonly unknown[] | undefined) =>
        Effect.gen(function* () {
          if (method === "inventory.equip") {
            const selector = args?.[0] as { readonly itemId: number };
            slotItemId = selector.itemId;
            equippedItemIds.push(selector.itemId);
            if (
              items.find((candidate) => candidate.itemId === selector.itemId)
                ?.category === "Item"
            ) {
              yield* confirmEquip({
                iRes: 1,
                o: { id: 400, cd: 10000, tgt: "h" },
              });
            } else {
              yield* PubSub.publish(packets, {
                command: "equipItem",
                direction: "extension",
                encoding: "json",
                raw: "",
                data: { ItemID: selector.itemId, uid: 1 },
              });
            }
            return Option.some(true);
          }
          if (method === "combat.getConsumableSkillItem") {
            return Option.some(
              slotItemId === undefined
                ? null
                : { itemId: slotItemId, ready: false },
            );
          }
          if (method === "inventory.unequipConsumable") {
            const selector = args?.[0] as { readonly itemId: number };
            unequippedItemIds.push(selector.itemId);
            slotItemId = undefined;
            return Option.some(true);
          }
          if (method === "inventory.use") {
            const selector = args?.[0] as { readonly itemId: number };
            const selected = state.items.inventory.find(
              (candidate) => candidate.itemId === selector.itemId,
            );
            if (selected === undefined) return Option.some(false);
            usedItemIds.push(selector.itemId);
            state.items.inventory = state.items.inventory.map((candidate) =>
              candidate === selected
                ? editItem(candidate, { quantity: candidate.quantity - 1 })
                : candidate,
            );
            return Option.some(true);
          }
          if (method === "player.getUserId") return Option.some(1);
          if (method === "player.isMember") return Option.some(true);
          return Option.none();
        }),
    } as unknown as BridgeService;
    const host = makeFakeHost(state, {
      swf: {
        "combat.getConsumableSkillItem": () =>
          slotItemId === undefined ? null : { itemId: slotItemId },
      } as unknown as Window["swf"],
    });
    let nextOperation = 0;
    vi.stubGlobal("__host", {
      prepareOperation: () => String(++nextOperation),
      operate: (id: string, json: string) => {
        const operation = JSON.parse(json) as { kind: string; selector: number };
        if (operation.kind !== "equipConsumable") {
          throw new Error("Unexpected operation");
        }
        Effect.runSync(
          bridge.invoke("inventory.equip", [{ itemId: operation.selector }], Schema.Boolean),
        );
        Effect.runSync(host.command({ kind: "refreshEquippedConsumable" }));
        completeOperation(id, { kind: "equipConsumable", value: true }, null);
      },
      cancelOperation: vi.fn(),
      acquireInventoryUseClaim: () => JSON.stringify("1"),
      releaseClaim: vi.fn(),
    });
    const house = makeHouse(bridge, host);
    const wait = {
      ...makeWait({
        subscribePackets: PubSub.subscribe(packets),
        subscribeEvents: PubSub.subscribe(events),
      }),
      forGameAction: () => Effect.succeed(true),
      isGameActionAvailable: () => Effect.succeed(true),
    } satisfies Wait;

    return {
      equippedItemIds,
      inventory: makeInventory(bridge, host, house, wait),
      unequippedItemIds,
      usedItemIds,
    };
  });

describe("Inventory.equipByEnhancement", () => {
  it.effect(
    "supports the enhancement selector forms accepted by enhanceItem",
    () =>
      Effect.gen(function* () {
        const best = equipment({
          enhancementLevel: 30,
          enhancementPatternId: 2,
          enhancementProcId: 14,
          equipmentSlot: "Weapon",
          itemId: 2,
        });
        const { equippedItemIds, inventory } = yield* makeHarness([
          equipment({
            enhancementLevel: 26,
            enhancementPatternId: 2,
            enhancementProcId: 14,
            equipmentSlot: "Weapon",
            itemId: 1,
          }),
          best,
          equipment({
            enhancementLevel: 35,
            enhancementPatternId: 10,
            enhancementProcId: 14,
            equipmentSlot: "Weapon",
            itemId: 3,
          }),
        ]);
        const selectors: readonly EquipEnhancementSelector[] = [
          { enhancement: "dauntless" },
          { enhancement: "forge", special: "dauntless" },
          { enhancement: "fighter", special: "dauntless" },
        ];

        for (const selector of selectors) {
          expect(yield* inventory.equipByEnhancement(selector)).toBe(true);
        }
        expect(equippedItemIds).toEqual([
          best.itemId,
          best.itemId,
          best.itemId,
        ]);
      }),
  );

  it.effect("filters by slot and ranks by enhancement level", () =>
    Effect.gen(function* () {
      const { equippedItemIds, inventory } = yield* makeHarness([
        equipment({
          enhancementLevel: 30,
          enhancementPatternId: 10,
          equipmentSlot: "he",
          itemId: 10,
        }),
        equipment({
          enhancementLevel: 40,
          enhancementPatternId: 10,
          equipmentSlot: "ba",
          itemId: 11,
        }),
        equipment({
          enhancementLevel: 35,
          enhancementPatternId: 10,
          equipmentSlot: "he",
          itemId: 12,
        }),
      ]);

      expect(
        yield* (
          inventory.equipByEnhancement as (
            selector: unknown,
          ) => Effect.Effect<boolean>
        )({
          enhancement: " forge ",
          slot: " HELM ",
        }),
      ).toBe(true);
      expect(equippedItemIds).toEqual([12]);
    }),
  );

  it.effect("rejects ambiguous or malformed selectors", () =>
    Effect.gen(function* () {
      const { equippedItemIds, inventory } = yield* makeHarness([
        equipment({
          enhancementLevel: 30,
          enhancementPatternId: 10,
          equipmentSlot: "he",
          itemId: 10,
        }),
      ]);
      const equipByEnhancement = inventory.equipByEnhancement as (
        selector: unknown,
      ) => Effect.Effect<boolean>;

      expect(yield* equipByEnhancement({ enhancement: "forge" })).toBe(false);
      expect(
        yield* equipByEnhancement({ enhancement: " ", slot: "helm" }),
      ).toBe(false);
      expect(
        yield* equipByEnhancement({
          enhancement: "forge",
          slot: "armor",
        }),
      ).toBe(false);
      expect(equippedItemIds).toEqual([]);
    }),
  );
});

describe("Inventory consumable equipment", () => {
  it.effect("projects local equip and unequip mutations", () =>
    Effect.gen(function* () {
      const first = consumable(100, "potion");
      const second = consumable(101, "scroll", true);
      const harness = yield* makeHarness([first, second]);

      expect(yield* harness.inventory.equip(first.itemId)).toBe(true);
      expect((yield* harness.inventory.get(first.itemId))?.equipped).toBe(true);
      expect((yield* harness.inventory.get(second.itemId))?.equipped).toBe(
        false,
      );
      expect(harness.equippedItemIds).toEqual([first.itemId]);

      expect(yield* harness.inventory.unequipConsumable(first.itemId)).toBe(
        true,
      );
      expect((yield* harness.inventory.get(first.itemId))?.equipped).toBe(
        false,
      );
      expect(harness.unequippedItemIds).toEqual([first.itemId]);
    }),
  );

  it.effect("uses but does not equip direct-use tonics", () =>
    Effect.gen(function* () {
      const tonic = consumable(102, "ToNiC");
      const harness = yield* makeHarness([tonic]);

      expect(yield* harness.inventory.equip(tonic.itemId)).toBe(false);
      expect(yield* harness.inventory.use(tonic.itemId)).toBe(true);
      const used = yield* harness.inventory.get(tonic.itemId);
      expect(used?.equipped).toBe(false);
      expect(used?.quantity).toBe(1);
      expect(harness.equippedItemIds).toEqual([]);
      expect(harness.usedItemIds).toEqual([tonic.itemId]);
    }),
  );

  it.effect("uses server-use boosts", () =>
    Effect.gen(function* () {
      const boost = consumable(103, "none", false, "ServerUse");
      const harness = yield* makeHarness([boost]);

      expect(yield* harness.inventory.use(boost.itemId)).toBe(true);
      expect((yield* harness.inventory.get(boost.itemId))?.quantity).toBe(1);
      expect(harness.usedItemIds).toEqual([boost.itemId]);
    }),
  );
});
