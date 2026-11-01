import {
  EntityState,
  LiveItem,
  LiveMonster,
  LivePlayer,
  toLiveMonster,
  type ItemContext,
  type ItemData,
  type ItemSnapshot,
  type MonsterData,
  type MonsterSnapshot,
  type PlayerData,
  type PlayerSnapshot,
  type QuestSnapshot,
  type ServerSnapshot,
  type ShopSnapshot,
} from "@lucent/game";
import type {
  AutoZoneConfig,
  Command,
  CommandOutputs,
  ItemContainer,
  ItemSelector,
  MapState,
  MonsterSelector,
  Query,
  QueryOutputs,
  ReadinessKey,
} from "@lucent/game/protocol";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";

import { makeApi } from "../api/Api";
import { Bridge, makeBridge } from "../bridge/Bridge";
import { Gateway, makeGateway } from "../bridge/Gateway";
import { HostState, type HostStateService } from "../state/HostState";
import { makeSettingsState, type SettingsState } from "../state/Settings";

const normalizeGameText = (value: string): string => value.trim().toLowerCase();

export interface FakeHostState {
  settings: SettingsState;
  autoZone: AutoZoneConfig;
  map: MapState;
  selfEntityId: number | null;
  players: PlayerSnapshot[];
  monsters: MonsterSnapshot[];
  cells: string[];
  cellPads: string[];
  items: Record<ItemContainer, ItemSnapshot[]>;
  hydration: Record<ItemContainer, number>;
  missing: ReadinessKey[];
  quests: QuestSnapshot[];
  acceptedQuestIds: number[];
  shop: ShopSnapshot | null;
  loggedIn: boolean;
  username: string;
  password: string;
  servers: ServerSnapshot[];
}

const readinessKeys: readonly ReadinessKey[] = [
  "houseInventory",
  "inventory",
  "map",
  "player",
];

export const makeFakeHostState = (): FakeHostState => ({
  settings: makeSettingsState(),
  autoZone: { enabled: false, map: null },
  map: { id: 0, name: "", roomNumber: 0 },
  selfEntityId: null,
  players: [],
  monsters: [],
  cells: [],
  cellPads: [],
  items: {
    bank: [],
    drop: [],
    house: [],
    inventory: [],
    shop: [],
    temporary: [],
  },
  hydration: {
    bank: 0,
    drop: 0,
    house: 0,
    inventory: 0,
    shop: 0,
    temporary: 0,
  },
  missing: [],
  quests: [],
  acceptedQuestIds: [],
  shop: null,
  loggedIn: false,
  username: "",
  password: "",
  servers: [],
});

export const item = (
  context: ItemContext,
  data: Partial<ItemData> & Pick<ItemData, "itemId" | "name">,
): ItemSnapshot =>
  new LiveItem({
    category: "",
    coins: false,
    context,
    cost: 0,
    description: "",
    equipmentSlot: "",
    equipped: false,
    file: "",
    houseItem: false,
    link: "",
    memberOnly: false,
    meta: "",
    quantity: 1,
    temporaryItem: false,
    ...data,
  }).toJSON();

export const monster = (
  data: Partial<MonsterData> & Pick<MonsterData, "monsterMapId" | "name">,
): MonsterSnapshot =>
  new LiveMonster({
    aggressive: false,
    cell: "Enter",
    hp: 100,
    level: 1,
    maxHp: 100,
    maxMp: 0,
    monsterId: 1,
    mp: 0,
    race: "None",
    state: EntityState.Idle,
    ...data,
  }).toJSON();

export const player = (
  data: Partial<PlayerData> & Pick<PlayerData, "entityId" | "username">,
): PlayerSnapshot =>
  new LivePlayer({
    afk: false,
    cell: "Enter",
    entityType: "player",
    hp: 100,
    level: 1,
    maxHp: 100,
    maxMp: 100,
    mp: 100,
    pad: "Spawn",
    position: { x: 0, y: 0 },
    state: EntityState.Idle,
    ...data,
  }).toJSON();

export const editItem = (
  entry: ItemSnapshot,
  patch: Partial<ItemData>,
): ItemSnapshot => item(patch.context ?? entry.context, { ...entry, ...patch });

export const moveItem = (
  state: FakeHostState,
  from: ItemContainer,
  to: ItemContainer,
  itemId: number,
): void => {
  const moved = state.items[from].find((entry) => entry.itemId === itemId);
  if (moved === undefined) return;
  state.items[from] = state.items[from].filter((entry) => entry !== moved);
  state.items[to] = [...state.items[to], editItem(moved, { context: to })];
};

const findItem = (
  state: FakeHostState,
  container: ItemContainer,
  selector: ItemSelector,
): ItemSnapshot | null => {
  const entries = state.items[container];
  const byName = (name: string) =>
    entries.find(
      (entry) => normalizeGameText(entry.name) === normalizeGameText(name),
    );
  const found =
    typeof selector === "number"
      ? entries.find((entry) => entry.itemId === selector)
      : typeof selector === "string"
        ? byName(selector)
        : "shopItemId" in selector
          ? entries.find((entry) =>
              container === "shop"
                ? entry.shopItemId === selector.shopItemId
                : entry.itemId === selector.shopItemId,
            )
          : "itemId" in selector
            ? entries.find((entry) => entry.itemId === selector.itemId)
            : byName(selector.name);
  return found ?? null;
};

const findMonster = (
  state: FakeHostState,
  selector: MonsterSelector,
): MonsterSnapshot | null =>
  state.monsters.find((entry) => toLiveMonster(entry).matches(selector)) ??
  null;

const findPlayer = (
  state: FakeHostState,
  selector: number | string,
): PlayerSnapshot | null =>
  state.players.find((entry) =>
    typeof selector === "number"
      ? entry.entityId === selector
      : normalizeGameText(entry.username) === normalizeGameText(selector),
  ) ?? null;

const answer = (state: FakeHostState, query: Query): unknown => {
  switch (query.kind) {
    case "environment":
      throw new Error("Environment state requires the native host");
    case "autoAttack":
      throw new Error("Auto attack requires the native host");
    case "combatState":
      throw new Error("Combat state requires the native host");
    case "autoRelogin":
      return { config: { enabled: false, delayMs: 3000, server: null }, status: { phase: { tag: "armed" }, attemptsRemaining: null, lastError: null, username: null, captured: false } };
    case "autoZone":
      return state.autoZone;
    case "settings":
      return state.settings;
    case "map":
      return state.map;
    case "me":
      return state.selfEntityId === null
        ? null
        : findPlayer(state, state.selfEntityId);
    case "selfEntityId":
      return state.selfEntityId;
    case "player":
      return findPlayer(state, query.selector);
    case "players":
      return state.players;
    case "playerAuras":
      return (findPlayer(state, query.entityId)?.auras ?? []).filter(
        (aura) => query.auraKind == null || aura.kind === query.auraKind,
      );
    case "monster":
      return findMonster(state, query.selector);
    case "monsters":
      return state.monsters;
    case "monsterAuras":
      return (
        findMonster(state, { monsterMapId: query.monsterMapId })?.auras ?? []
      ).filter(
        (aura) => query.auraKind == null || aura.kind === query.auraKind,
      );
    case "monsterDrops": {
      const found = findMonster(state, { monsterMapId: query.monsterMapId });
      return found === null || found.drops.length === 0 ? null : found.drops;
    }
    case "cells":
      return state.cells;
    case "cellPads":
      return state.cellPads;
    case "item":
      return findItem(state, query.container, query.selector);
    case "items":
      return state.items[query.container];
    case "itemHydration":
      return state.hydration[query.container];
    case "readiness": {
      const flags = Object.fromEntries(
        readinessKeys.map((key) => [key, !state.missing.includes(key)]),
      );
      return { epoch: 0, failures: {}, missing: state.missing, state: flags };
    }
    case "quest":
      return state.quests.find((quest) => quest.id === query.id) ?? null;
    case "quests":
      return state.quests;
    case "acceptedQuests":
      return state.acceptedQuestIds.flatMap((id) =>
        state.quests.filter((quest) => quest.id === id),
      );
    case "shop":
      return state.shop;
    case "auth":
      return {
        loggedIn: state.loggedIn,
        password: state.password,
        servers: state.servers,
        username: state.username,
      };
  }
};

type Loader = Record<string, ((...args: never[]) => unknown) | undefined>;

const run = (state: FakeHostState, loader: Loader, command: Command) => {
  const call = (name: string, ...args: unknown[]): unknown => {
    const method = loader[name];
    if (method === undefined) throw new Error(`Missing loader method ${name}`);
    return Reflect.apply(method, loader, args);
  };
  const attempt = (name: string, ...args: unknown[]): unknown => {
    try {
      return call(name, ...args);
    } catch {
      return undefined;
    }
  };

  switch (command.kind) {
    case "updateEnvironment":
      throw new Error("Environment mutations require the native host");
    case "refreshCombatState":
      throw new Error("Combat state hydration requires the native host");
    case "enableAutoAttack":
    case "disableAutoAttack":
    case "setAutoReloginEnabled":
    case "setAutoReloginDelay":
    case "setAutoReloginServer":
      throw new Error(`FakeHost does not implement ${command.kind}`);
    case "patchAutoZone":
      state.autoZone = {
        enabled: command.patch.enabled ?? state.autoZone.enabled,
        map: command.patch.map === undefined ? state.autoZone.map : command.patch.map,
      };
      return state.autoZone;
    case "updateSettings":
      return state.settings;
    case "reapplySettings":
      return null;
    case "refreshContainer": {
      const answer = call(`${command.container}.getItems`) as ItemSnapshot[];
      state.items[command.container] = answer.filter(
        (entry) => command.container !== "bank" || entry.category !== "Class",
      );
      state.hydration[command.container] += 1;
      return state.items[command.container].length;
    }
    case "confirmShopClosed": {
      const open = call(
        "shops.isOpen",
        ...(command.shopId == null ? [] : [command.shopId]),
      ) as boolean;
      if (
        !open &&
        (command.shopId == null || state.shop?.id === command.shopId)
      ) {
        state.shop = null;
        state.items.shop = [];
        state.hydration.shop += 1;
      }
      return !open;
    }
    case "rejectDrop": {
      const drop = findItem(state, "drop", command.selector);
      if (drop === null) return false;
      const rejected = call("drops.reject", drop.itemId) as boolean;
      if (rejected) {
        state.items.drop = state.items.drop.filter((entry) => entry !== drop);
      }
      return rejected;
    }
    case "refreshCells":
      if (call("world.isLoaded") === true) {
        state.cells = call("world.getCells") as string[];
      }
      return state.cells;
    case "refreshCellPads":
      if (call("world.isLoaded") === true) {
        state.cellPads = call("world.getCellPads") as string[];
      }
      return state.cellPads;
    case "hydrateMonster": {
      const current = findMonster(state, command.selector);
      if (current !== null) return current;
      const hydrated = call(
        "world.getMonster",
        command.selector,
      ) as MonsterSnapshot | null;
      if (hydrated !== null) state.monsters = [...state.monsters, hydrated];
      return hydrated;
    }
    case "login": {
      const username = command.username.trim();
      if (username === "" || command.password === "") return false;
      const previous = { password: state.password, username: state.username };
      state.username = username;
      state.password = command.password;
      try {
        call("auth.login", username, command.password);
      } catch (cause) {
        state.username = previous.username;
        state.password = previous.password;
        throw cause;
      }
      return true;
    }
    case "logout":
      try {
        call("auth.logout");
      } finally {
        state.loggedIn = false;
        state.username = "";
        state.password = "";
        state.servers = [];
      }
      return null;
    case "refreshCredentials":
      if (state.missing.length === 0) {
        const username = attempt(
          "flash.getGameObjectS",
          "loginInfo.strUsername",
        );
        const password = attempt(
          "flash.getGameObjectS",
          "loginInfo.strPassword",
        );
        if (typeof username === "string" && username.trim() !== "") {
          state.username = username.trim();
        }
        if (typeof password === "string" && password !== "") {
          state.password = password;
        }
      }
      return { password: state.password, username: state.username };
    case "refreshServers": {
      const servers = attempt("auth.getServers") as
        ServerSnapshot[] | null | undefined;
      if (servers != null) state.servers = servers;
      return state.servers;
    }
    case "refreshLoggedIn": {
      const loggedIn = attempt("auth.isLoggedIn");
      if (typeof loggedIn === "boolean") state.loggedIn = loggedIn;
      return state.loggedIn;
    }
    case "refreshAcceptedQuests":
      state.acceptedQuestIds = call("quests.getAccepted") as number[];
      return answer(state, { kind: "acceptedQuests" });
    case "refreshEquippedConsumable": {
      const slot = call("combat.getConsumableSkillItem") as {
        readonly itemId: number;
      } | null;
      const itemId = slot?.itemId ?? null;
      state.items.inventory = state.items.inventory.map((entry) => {
        const link = entry.link.trim().toLowerCase();
        return entry.category === "Item" && link !== "" && link !== "none"
          ? editItem(entry, { equipped: entry.itemId === itemId })
          : entry;
      });
      return itemId;
    }
  }
};

const copy = (value: unknown): never =>
  (value === undefined
    ? undefined
    : JSON.parse(JSON.stringify(value))) as never;

export const makeFakeHost = (
  state: FakeHostState,
  target: Pick<Window, "swf">,
): HostStateService => ({
  command: <K extends Command["kind"]>(
    command: Extract<Command, { kind: K }>,
  ) =>
    Effect.sync((): Option.Option<CommandOutputs[K]> => {
      try {
        return Option.some(
          copy(run(state, target.swf as unknown as Loader, command)),
        );
      } catch {
        return Option.none();
      }
    }),
  read: <K extends Query["kind"]>(query: Extract<Query, { kind: K }>) =>
    Effect.sync((): QueryOutputs[K] => copy(answer(state, query))),
});

export const emitExtension = (target: Window, dataObj: object): void => {
  target.onExtensionResponse?.(JSON.stringify({ dataObj, type: "json" }));
};

export const makeFakeApi = (target: Window, state: FakeHostState) =>
  Effect.gen(function* () {
    const bridge = yield* makeBridge(target);
    const gateway = yield* makeGateway(target).pipe(
      Effect.provideService(Bridge, bridge),
    );
    return yield* makeApi.pipe(
      Effect.provideService(Bridge, bridge),
      Effect.provideService(Gateway, gateway),
      Effect.provideService(HostState, makeFakeHost(state, target)),
    );
  });
