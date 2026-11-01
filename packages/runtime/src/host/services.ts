import { makeEnvironmentService } from "./environment";
import * as Effect from "effect/Effect";

import { ArmyError, ArmyLoopTauntError } from "../game/army/ArmyErrors";
import type { ScriptDialogsShape } from "../game/scripting/ScriptDialogs";
import type { ScriptServicesShape } from "../game/scripting/ScriptServices";

export class HostServiceUnavailable extends Error {
  constructor(readonly service: string) {
    super(`The ${service} service is not available yet.`);
    this.name = "HostServiceUnavailable";
  }
}

const defect = (service: string) => () =>
  Effect.die(new HostServiceUnavailable(service));
const failure = (service: string) => () =>
  Effect.fail(new HostServiceUnavailable(service));
const armyFailure =
  <E>(ErrorClass: new (message: string, cause: unknown) => E) => () => {
    const cause = new HostServiceUnavailable("army");
    return Effect.fail(new ErrorClass(cause.message, cause));
  };

const unavailableEnvironment = (): never => { throw new HostServiceUnavailable("environment"); };
const environmentHost = typeof __host === "undefined"
  ? { read: unavailableEnvironment, command: unavailableEnvironment }
  : { read: __host.read, command: __host.command };

export const unavailableScriptServices = {
  army: {
    equipSet: armyFailure(ArmyError),
    getConfigString: defect("army"),
    getConfigValue: defect("army"),
    getPlayerNumber: defect("army"),
    getSession: defect("army"),
    isLeader: defect("army"),
    isMember: defect("army"),
    isStarted: defect("army"),
    joinMap: armyFailure(ArmyError),
    kill: armyFailure(ArmyError),
    killForItem: armyFailure(ArmyError),
    killForTempItem: armyFailure(ArmyError),
    leave: defect("army"),
    loopTaunt: armyFailure(ArmyLoopTauntError),
    runStep: armyFailure(ArmyError),
    start: armyFailure(ArmyError),
    sync: armyFailure(ArmyError),
    waitForAllInMap: armyFailure(ArmyError),
  },
  environment: makeEnvironmentService(environmentHost, {
    fetchBoosts: failure("environment"),
    syncToAll: failure("environment"),
  }),
} satisfies Omit<ScriptServicesShape, "autoZone" | "autoRelogin" | "fileSystem" | "http">;

export const unavailableScriptDialogs = {
  alert: defect("dialogs"),
  beep: defect("beep"),
  confirm: defect("dialogs"),
  getCurrent: defect("dialogs"),
  onCurrent: defect("dialogs"),
  prompt: defect("dialogs"),
  respond: defect("dialogs"),
} satisfies ScriptDialogsShape;
