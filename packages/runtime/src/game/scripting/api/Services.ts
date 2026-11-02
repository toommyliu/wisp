import type { ArmyApiRuntimeShape } from "../../army/Army";
import type { ApiService } from "../../flash/api/Api";
import type { EnvironmentShape } from "../../../host/environment";

export interface ScriptRuntimeServices {
  readonly army: ArmyApiRuntimeShape;
  readonly auth: ApiService["auth"];
  readonly bank: ApiService["bank"];
  readonly combat: ApiService["combat"];
  readonly drops: ApiService["drops"];
  readonly environment: EnvironmentShape;
  readonly events: ApiService["events"];
  readonly house: ApiService["house"];
  readonly inventory: ApiService["inventory"];
  readonly map: ApiService["map"];
  readonly monsters: ApiService["monsters"];
  readonly packet: ApiService["packet"];
  readonly player: ApiService["player"];
  readonly players: ApiService["players"];
  readonly quests: ApiService["quests"];
  readonly settings: ApiService["settings"];
  readonly shops: ApiService["shops"];
  readonly tempInventory: ApiService["tempInventory"];
  readonly wait: ApiService["wait"];
}

export const makeScriptRuntimeServices = (
  api: ApiService,
  army: ArmyApiRuntimeShape,
  environment: EnvironmentShape,
): ScriptRuntimeServices => ({ ...api, army, environment });
