import * as Effect from "effect/Effect";

import type { ArmyApiRuntimeShape } from "../game/army/Army";
import { ArmyError, ArmyLoopTauntError } from "../game/army/ArmyErrors";
import type { EnvironmentShape } from "./environment";

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

export const unavailableArmy: ArmyApiRuntimeShape = {
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
};

export const unavailableEnvironment: Pick<
  EnvironmentShape,
  "fetchBoosts" | "syncToAll"
> = {
  fetchBoosts: failure("environment"),
  syncToAll: failure("environment"),
};
