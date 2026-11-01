import * as Effect from "effect/Effect";
import * as Random from "effect/Random";

import {
  MAXIMUM_ROOM_NUMBER,
  MINIMUM_PRIVATE_ROOM,
  MINIMUM_RANDOM_PRIVATE_ROOM,
  MINIMUM_ROOM_NUMBER,
  type RoomPolicy,
} from "@lucent/schemas/accountSettings";

export const minimumPrivateRoom = MINIMUM_PRIVATE_ROOM;
export const minimumRandomPrivateRoom = MINIMUM_RANDOM_PRIVATE_ROOM;
export const maximumRoom = MAXIMUM_ROOM_NUMBER;

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

export const isPrivateRoom = (room: number): boolean =>
  Number.isSafeInteger(room) &&
  room >= minimumPrivateRoom &&
  room <= maximumRoom;

export const isRoomNumber = (room: number): boolean =>
  Number.isSafeInteger(room) &&
  room >= MINIMUM_ROOM_NUMBER &&
  room <= maximumRoom;

export const hasFixedRoom = (map: string): boolean => {
  const roomToken = splitRoom(map).roomToken;
  return roomToken !== undefined && parseRoom(roomToken) !== undefined;
};

export const withRoomNumber = (map: string, room: number): string => {
  const target = splitRoom(map);
  if (
    target.roomToken !== undefined &&
    parseRoom(target.roomToken) !== undefined
  ) {
    return target.map;
  }
  return `${target.name}-${Math.max(1, Math.trunc(room))}`;
};

export const applyRoomPolicy = Effect.fn("applyRoomPolicy")(function* (
  map: string,
  policy: RoomPolicy,
) {
  const trimmed = map.trim();
  if (trimmed === "" || policy.kind === "public") return map;
  const room =
    policy.kind === "specific" ? policy.roomNumber : yield* randomPrivateRoom;
  return withRoomNumber(trimmed, room);
});

export const roomPolicyAcceptsRoom = (
  policy: RoomPolicy,
  roomNumber: number,
): boolean =>
  policy.kind === "public" ||
  (policy.kind === "random-private"
    ? isPrivateRoom(roomNumber)
    : roomNumber === policy.roomNumber);
