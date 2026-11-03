import * as Effect from "effect/Effect";

import type { GatewayService } from "../bridge/Gateway";
import type { HostStateService } from "../state/HostState";
import type { Events } from "./Events";
import type { MonsterServices } from "./Monsters";

interface Services {
  readonly events: Events;
  readonly gateway: GatewayService;
  readonly host: HostStateService;
  readonly monsters: MonsterServices;
}

export const start = Effect.fnUntraced(function* ({
  events,
  gateway,
  host,
  monsters,
}: Services) {
  const ensureCurrentCell = () =>
    monsters.lookup.getAll().pipe(Effect.flatMap(monsters.ensureCurrentCell));

  yield* events.on({ type: "join-map" }, ensureCurrentCell);
  yield* events.on({ type: "player-location" }, (event) =>
    host
      .read({ kind: "me" })
      .pipe(
        Effect.flatMap((player) =>
          player?.entityId === event.entityId
            ? ensureCurrentCell()
            : Effect.void,
        ),
      ),
  );
  yield* gateway.start();
});
