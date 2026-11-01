import * as Effect from "effect/Effect";

import type {
  ScriptAuthApi,
  ScriptBankApi,
  ScriptDropsApi,
  ScriptHouseApi,
  ScriptInventoryApi,
  ScriptMapApi,
  ScriptMonstersApi,
  ScriptQuestsApi,
  ScriptTempInventoryApi,
  ScriptWaitApi,
} from "@lucent/scripting-api/contract";
import type { ScriptRuntimeServices } from "./Services";

export interface ScriptPublicServices {
  readonly auth: ScriptAuthApi;
  readonly bank: ScriptBankApi;
  readonly drops: ScriptDropsApi;
  readonly house: ScriptHouseApi;
  readonly inventory: ScriptInventoryApi;
  readonly map: ScriptMapApi;
  readonly monsters: ScriptMonstersApi;
  readonly quests: ScriptQuestsApi;
  readonly tempInventory: ScriptTempInventoryApi;
  readonly wait: ScriptWaitApi;
}

export const makeScriptPublicServices = (
  services: ScriptRuntimeServices,
): ScriptPublicServices => ({
  auth: Object.freeze({
    connectTo: services.auth.connectTo,
    getPassword: services.auth.getPassword,
    getServers: services.auth.getServers,
    getUsername: services.auth.getUsername,
    isLoggedIn: services.auth.isLoggedIn,
    isServerSelectReady: services.auth.isServerSelectReady,
    isTemporarilyKicked: services.auth.isTemporarilyKicked,
    login: services.auth.login,
    logout: () => services.auth.logout().pipe(Effect.asVoid),
  }),
  bank: Object.freeze({
    contains: services.bank.contains,
    deposit: services.bank.deposit,
    depositBatch: services.bank.depositBatch,
    get: services.bank.get,
    getAll: services.bank.getAll,
    getAvailableSlots: services.bank.getAvailableSlots,
    getSlots: services.bank.getSlots,
    getUsedSlots: services.bank.getUsedSlots,
    isOpen: services.bank.isOpen,
    load: services.bank.load,
    open: services.bank.open,
    swap: services.bank.swap,
    withdraw: services.bank.withdraw,
    withdrawBatch: services.bank.withdrawBatch,
  }),
  drops: Object.freeze({
    accept: services.drops.accept,
    contains: services.drops.contains,
    get: services.drops.get,
    getAll: services.drops.getAll,
    reject: services.drops.reject,
  }),
  house: Object.freeze({
    contains: services.house.contains,
    get: services.house.get,
    getAll: services.house.getAll,
    getAvailableSlots: services.house.getAvailableSlots,
    getSlots: services.house.getSlots,
    getUsedSlots: services.house.getUsedSlots,
  }),
  inventory: Object.freeze({
    contains: services.inventory.contains,
    equip: services.inventory.equip,
    equipByEnhancement: services.inventory.equipByEnhancement,
    get: services.inventory.get,
    getAll: services.inventory.getAll,
    getAvailableSlots: services.inventory.getAvailableSlots,
    getSlots: services.inventory.getSlots,
    getUsedSlots: services.inventory.getUsedSlots,
    unequipConsumable: services.inventory.unequipConsumable,
    use: services.inventory.use,
    wear: services.inventory.wear,
  }),
  map: Object.freeze({
    getCellPads: services.map.getCellPads,
    getCells: services.map.getCells,
    getId: services.map.getId,
    getMapItem: services.map.getMapItem,
    getName: services.map.getName,
    getRoomNumber: services.map.getRoomNumber,
    isLoaded: services.map.isLoaded,
    loadSwf: services.map.loadSwf,
    reload: services.map.reload,
    setSpawnPoint: services.map.setSpawnPoint,
  }),
  monsters: Object.freeze({
    get: services.monsters.get,
    getAll: services.monsters.getAll,
    getAvailable: services.monsters.getAvailable,
    isAvailable: services.monsters.isAvailable,
  }),
  quests: Object.freeze({
    abandon: services.quests.abandon,
    accept: services.quests.accept,
    acceptBatch: services.quests.acceptBatch,
    canComplete: services.quests.canComplete,
    complete: services.quests.complete,
    get: services.quests.get,
    getAccepted: services.quests.getAccepted,
    getAll: services.quests.getAll,
    getMaxTurnIns: services.quests.getMaxTurnIns,
    isAvailable: services.quests.isAvailable,
    isInProgress: services.quests.isInProgress,
    load: services.quests.load,
    loadBatch: services.quests.loadBatch,
  }),
  tempInventory: Object.freeze({
    contains: services.tempInventory.contains,
    get: services.tempInventory.get,
    getAll: services.tempInventory.getAll,
  }),
  wait: Object.freeze({
    forGameAction: services.wait.forGameAction,
    isGameActionAvailable: services.wait.isGameActionAvailable,
    until: services.wait.until,
    untilSome: services.wait.untilSome,
  }),
});
