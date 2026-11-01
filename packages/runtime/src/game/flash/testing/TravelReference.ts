import { EntityState } from "@lucent/game";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";
import type * as Duration from "effect/Duration";
import type { BridgeService } from "../bridge/Bridge";
import type { HostStateService } from "../state/HostState";
import type { Player } from "../api/Player";
import type { Map, CellPositionOptions } from "../api/Map";
import type { Wait } from "../api/Wait";
import { stopCombat as stopCombatControl } from "../api/internal/CombatControl";
import { parseMapTarget } from "./TravelTargetReference";
import { combatExitCells } from "./CombatExitReference";

const sameText = (left: string, right: string) => left.localeCompare(right, undefined, { sensitivity: "accent" }) === 0;
const maxCombatExitJumps = 3;
export const makeTravelReference = (bridge: BridgeService, host: HostStateService, map: Map, wait: Wait, player: Player) => {
  const { getState, getCell, getPad, isAlive, isReady } = player;
  const readLiveLocation = () =>
    Effect.all({
      cell: bridge
        .invoke("player.getCell", undefined, Schema.String)
        .pipe(Effect.map(Option.getOrElse(() => ""))),
      pad: bridge
        .invoke("player.getPad", undefined, Schema.String)
        .pipe(Effect.map(Option.getOrElse(() => ""))),
    });

  const isCellReady = (cell: string, pad?: string) => {
    const args: Parameters<Window["swf"]["world.isCellReady"]> =
      pad === undefined ? [cell] : [cell, pad];
    return bridge
      .invoke("world.isCellReady", args, Schema.Boolean)
      .pipe(Effect.map(Option.getOrElse(() => false)));
  };

  const jumpToCell = Effect.fn("Player.jumpToCell")(function* (
    cell: string,
    pad?: string,
  ) {
    const targetCell = cell.trim();
    if (targetCell === "") return false;
    const args: Parameters<Window["swf"]["player.jump"]> =
      pad === undefined ? [targetCell] : [targetCell, pad];
    const invoked = yield* bridge.invoke("player.jump", args, Schema.Void);
    if (Option.isNone(invoked)) return false;

    let requestedLocationObserved = false;
    let settledAtRequestedLocation = false;
    const settled = yield* wait.until(
      Effect.gen(function* () {
        const observed = yield* readLiveLocation();
        const atRequestedLocation =
          sameText(observed.cell, targetCell) &&
          (pad === undefined || sameText(observed.pad, pad));
        // Do not detect a redirect until AQW has reported the requested location.
        if (atRequestedLocation) requestedLocationObserved = true;
        if (!requestedLocationObserved) return false;

        const readinessCell = atRequestedLocation
          ? targetCell
          : observed.cell.trim();
        const readinessPad = atRequestedLocation
          ? pad
          : observed.pad.trim() || undefined;
        if (readinessCell === "") return false;

        // AQW updates strFrame before its cell timeline and display list settle.
        const ready = yield* isCellReady(readinessCell, readinessPad);
        if (ready) settledAtRequestedLocation = atRequestedLocation;
        return ready;
      }),
      { timeout: "3 seconds" },
    );
    if (!settled) return false;
    return settledAtRequestedLocation;
  });

  const stopCombat = stopCombatControl(bridge);
  const waitUntilIdle = (timeout: Duration.Input) =>
    wait.until(
      getState().pipe(
        Effect.flatMap((state) =>
          state !== EntityState.Idle
            ? Effect.succeed(false)
            : Effect.sleep("500 millis").pipe(
                Effect.andThen(getState()),
                Effect.map((confirmed) => confirmed === EntityState.Idle),
              ),
        ),
      ),
      { interval: "100 millis", timeout },
    );

  const exitCombat = Effect.fn("Player.exitCombat")(function* () {
    if ((yield* getState()) === EntityState.Idle) return true;
    const currentCell = yield* getCell();
    const currentPad = yield* getPad();
    const cells = combatExitCells(
      yield* map.getCells(),
      yield* host.read({ kind: "monsters" }),
      currentCell,
    );

    for (const cell of cells.slice(0, maxCombatExitJumps)) {
      yield* stopCombat;
      yield* jumpToCell(
        cell,
        sameText(cell, currentCell) ? currentPad : undefined,
      );
      if (yield* waitUntilIdle("2 seconds")) return true;
    }
    return false;
  });

  const joinMap = (target: string, options?: CellPositionOptions) =>
    Effect.gen(function* () {
      const { cell, pad } = options ?? {};
      const destination = yield* parseMapTarget(target);
      if (destination.map === "") return false;
      const targetCell = cell ?? (pad === undefined ? undefined : "Enter");
      const currentMap = yield* host.read({ kind: "map" });
      const alreadyLoaded =
        (yield* map.isLoaded()) &&
        sameText(currentMap.name, destination.name) &&
        (!destination.requireExactRoom ||
          currentMap.roomNumber === destination.roomNumber);
      if (alreadyLoaded) {
        if (!(yield* wait.until(isReady(), { timeout: "10 seconds" })))
          return false;
        return targetCell === undefined
          ? true
          : yield* jumpToCell(targetCell, pad);
      }
      if (!(yield* wait.until(isReady(), { timeout: "10 seconds" })))
        return false;
      if (!(yield* wait.forGameAction("tfer", { timeout: "10 seconds" })))
        return false;
      if (!(yield* wait.until(isAlive(), { timeout: "15 seconds" })))
        return false;
      if (!(yield* exitCombat())) return false;
      const args: Parameters<Window["swf"]["player.joinMap"]> =
        cell === undefined && pad === undefined
          ? [destination.map]
          : pad === undefined
            ? [destination.map, cell ?? "Enter"]
            : [destination.map, cell ?? "Enter", pad];
      if (
        Option.isNone(yield* bridge.invoke("player.joinMap", args, Schema.Void))
      )
        return false;
      const loaded = yield* wait.until(
        host
          .read({ kind: "map" })
          .pipe(
            Effect.flatMap((current) =>
              map
                .isLoaded()
                .pipe(
                  Effect.map(
                    (ready) =>
                      ready &&
                      sameText(current.name, destination.name) &&
                      (!destination.requireExactRoom ||
                        current.roomNumber === destination.roomNumber),
                  ),
                ),
            ),
          ),
        { timeout: "20 seconds" },
      );
      if (!loaded) return false;
      if (!(yield* wait.until(isReady(), { timeout: "10 seconds" })))
        return false;
      if (targetCell === undefined) return true;
      return yield* jumpToCell(targetCell, pad);
    });

  return { jumpToCell, joinMap, exitCombat };
};
