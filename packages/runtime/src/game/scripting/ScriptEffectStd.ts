import * as Duration from "effect/Duration";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import { pipe } from "effect/Function";

import type {
  ScriptDurationModule,
  ScriptEffectAllTuple,
  ScriptEffectModule,
  ScriptEffectStd,
  ScriptOptionModule,
} from "@lucent/scripting-api/contract";

const scriptDurationModule: ScriptDurationModule = {
  days: Duration.days,
  hours: Duration.hours,
  millis: Duration.millis,
  minutes: Duration.minutes,
  seconds: Duration.seconds,
  toMillis: Duration.toMillis,
};
Object.freeze(scriptDurationModule);

function scriptEffectAll<
  const Effects extends readonly Effect.Effect<unknown, unknown, unknown>[],
>(effects: Effects): ScriptEffectAllTuple<Effects>;
function scriptEffectAll<Value, Error, Requirements>(
  effects: Iterable<Effect.Effect<Value, Error, Requirements>>,
): Effect.Effect<readonly Value[], Error, Requirements>;
function scriptEffectAll(
  effects: Iterable<Effect.Effect<unknown, unknown, unknown>>,
): Effect.Effect<readonly unknown[], unknown, unknown> {
  return Effect.all(effects);
}

const scriptEffectModule: ScriptEffectModule = {
  all: scriptEffectAll,
  as: Effect.as,
  asVoid: Effect.asVoid,
  catch: Effect.catch,
  fail: Effect.fail,
  flatMap: Effect.flatMap,
  forEach: (values, transform) => Effect.forEach(values, transform),
  gen: Effect.gen,
  map: Effect.map,
  mapError: Effect.mapError,
  raceFirst: Effect.raceFirst,
  sleep: Effect.sleep,
  succeed: Effect.succeed,
  sync: Effect.sync,
  tap: Effect.tap,
  timeoutOption: Effect.timeoutOption,
  try: Effect.try,
  tryPromise: Effect.tryPromise,
  void: Effect.void,
};
Object.freeze(scriptEffectModule);

const scriptOptionModule: ScriptOptionModule = {
  getOrElse: Option.getOrElse,
  isNone: Option.isNone,
  isSome: Option.isSome,
  map: Option.map,
  match: Option.match,
  none: Option.none,
  some: Option.some,
};
Object.freeze(scriptOptionModule);

export const scriptEffectStd: ScriptEffectStd = Object.freeze({
  Duration: scriptDurationModule,
  Effect: scriptEffectModule,
  Option: scriptOptionModule,
  pipe,
});
