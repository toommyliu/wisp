import type { ItemQuery } from "@lucent/game";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { operate } from "../../../host/operations";
import type { BridgeService } from "../bridge/Bridge";
import {
  isSelectable,
  readItem,
  readItems,
  type HostStateService,
} from "../state/HostState";
import type { Auth } from "./Auth";

export const makeDrops = Effect.fnUntraced(function* (
  bridge: BridgeService,
  host: HostStateService,
  auth: Auth,
) {
  const operationHost = globalThis.__host;
  const get = (selector: ItemQuery) => readItem(host, "drop", selector);

  const accept = (selector: ItemQuery) =>
    isSelectable(selector)
      ? operate({ kind: "acceptDrop", selector }, operationHost).pipe(
          Effect.map((answer) => answer.kind === "acceptDrop" && answer.value),
          Effect.catch(() => Effect.succeed(false)),
        )
      : Effect.succeed(false);

  const contains = (selector: ItemQuery) =>
    get(selector).pipe(Effect.map((drop) => drop !== null));

  const getAll = () => readItems(host, "drop");

  const isCustomUiEnabled = () =>
    bridge
      .invoke("drops.isUsingCustomDrops", undefined, Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));

  const reject = (selector: ItemQuery) =>
    Effect.gen(function* () {
      if (!(yield* auth.isLoggedIn())) return false;
      if (!isSelectable(selector)) return false;
      const rejected = yield* host.command({ kind: "rejectDrop", selector });
      return Option.getOrElse(rejected, () => false);
    });

  const toggleUi = () =>
    bridge.invoke("drops.toggleUi", undefined, Schema.Void).pipe(Effect.asVoid);

  return {
    accept,
    contains,
    get,
    getAll,
    isCustomUiEnabled,
    reject,
    toggleUi,
  };
});

export type Drops = Effect.Success<ReturnType<typeof makeDrops>>;
