import { toPlayerSelector } from "@lucent/game";

import type { ApiService } from "../../flash/api/Api";
import type { ScriptPlayersApi } from "@lucent/scripting-api/contract";

export const makeScriptPlayersApi = (
  players: Pick<ApiService["players"], "get" | "getAll">,
): ScriptPlayersApi =>
  Object.freeze({
    get: (query: Parameters<ScriptPlayersApi["get"]>[0]) =>
      players.get(toPlayerSelector(query).username),
    getAll: players.getAll,
  });
