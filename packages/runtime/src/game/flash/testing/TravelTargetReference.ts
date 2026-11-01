import * as Effect from "effect/Effect";
import * as Random from "effect/Random";

import {
  MAXIMUM_ROOM_NUMBER,
  MINIMUM_PRIVATE_ROOM,
  MINIMUM_RANDOM_PRIVATE_ROOM,
} from "@lucent/schemas/accountSettings";

export const minimumPrivateRoom = MINIMUM_PRIVATE_ROOM;
export const minimumRandomPrivateRoom = MINIMUM_RANDOM_PRIVATE_ROOM;
export const maximumRoom = MAXIMUM_ROOM_NUMBER;

export interface MapTarget {
  readonly map: string;
  readonly name: string;
  readonly requireExactRoom: boolean;
  readonly roomNumber?: number;
}

interface MapParts {
  readonly map: string;
  readonly name: string;
  readonly roomToken?: string;
}

const splitRoom = (map: string): MapParts => {
  const trimmed = map.trim();
  const separator = trimmed.indexOf("-");
  if (separator <= 0 || separator === trimmed.length - 1) {
    return { map: trimmed, name: trimmed };
  }

  return {
    map: trimmed,
    name: trimmed.slice(0, separator),
    roomToken: trimmed.slice(separator + 1),
  };
};

const parseRoom = (value: string): number | undefined => {
  if (!/^\d+$/u.test(value)) return undefined;
  const room = Number(value);
  return Number.isSafeInteger(room) && room >= 1 ? room : undefined;
};

export const randomPrivateRoom = Random.nextIntBetween(
  minimumRandomPrivateRoom,
  maximumRoom,
);

export const parseMapTarget = (map: string): Effect.Effect<MapTarget> =>
  Effect.gen(function* () {
    const target = splitRoom(map);
    if (target.roomToken === undefined) {
      return {
        map: target.map,
        name: target.name,
        requireExactRoom: false,
      };
    }

    const fixedRoom = parseRoom(target.roomToken);
    const roomNumber = fixedRoom ?? (yield* randomPrivateRoom);
    const requireExactRoom =
      fixedRoom === undefined ||
      (fixedRoom >= minimumPrivateRoom && fixedRoom <= maximumRoom);
    return {
      map:
        fixedRoom === undefined ? `${target.name}-${roomNumber}` : target.map,
      name: target.name,
      requireExactRoom,
      roomNumber,
    };
  });
