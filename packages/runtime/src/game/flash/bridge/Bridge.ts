import * as Data from "effect/Data";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Schema from "effect/Schema";

import { DiagnosticSink } from "./DiagnosticSink";

type Method = keyof Window["swf"];
type MethodArguments<M extends Method> = Readonly<Parameters<Window["swf"][M]>>;

class InvokeFailure extends Data.TaggedError("InvokeFailure")<{
  readonly cause: unknown;
}> {}

export const makeBridge = (target?: Pick<Window, "swf">) =>
  Effect.gen(function* () {
    const swf = (target ?? window).swf;
    const diagnosticSink = yield* DiagnosticSink;

    const fail = (
      method: Method,
      cause: unknown,
      args: readonly unknown[] | undefined,
    ) =>
      Effect.sync(() =>
        diagnosticSink.report(
          "invoke",
          String(method),
          cause,
          method === "auth.login" ? undefined : args,
        ),
      ).pipe(Effect.as(Option.none()));

    const invoke = <M extends Method, A>(
      method: M,
      args: MethodArguments<M> | undefined,
      schema: Schema.Decoder<A>,
    ): Effect.Effect<Option.Option<A>> =>
      Effect.try({
        try: () => {
          const candidate: unknown = swf[method];
          if (typeof candidate !== "function") {
            throw new Error(`Missing Flash method: ${String(method)}`);
          }
          return Reflect.apply(candidate, swf, [...(args ?? [])]);
        },
        catch: (cause) => new InvokeFailure({ cause }),
      }).pipe(
        Effect.flatMap(Schema.decodeUnknownEffect(schema)),
        Effect.map(Option.some),
        Effect.catch((cause) => fail(method, cause, args)),
      );

    const invokeJson = <M extends Method, A>(
      method: M,
      args: MethodArguments<M> | undefined,
      schema: Schema.Decoder<A>,
    ): Effect.Effect<Option.Option<A>> =>
      invoke(method, args, Schema.Union([Schema.String, Schema.Unknown])).pipe(
        Effect.flatMap(
          Option.match({
            onNone: () => Effect.succeed(Option.none<A>()),
            onSome: (value) => {
              if (typeof value !== "string") {
                return Schema.decodeUnknownEffect(schema)(value).pipe(
                  Effect.map(Option.some),
                  Effect.catch((cause) => fail(method, cause, args)),
                );
              }

              return Effect.try({
                try: () => JSON.parse(value) as unknown,
                catch: (cause) => new InvokeFailure({ cause }),
              }).pipe(
                Effect.flatMap(Schema.decodeUnknownEffect(schema)),
                Effect.map(Option.some),
                Effect.catch((cause) => fail(method, cause, args)),
              );
            },
          }),
        ),
      );

    return {
      invoke,
      invokeJson,
    };
  });

export type BridgeService = Effect.Success<ReturnType<typeof makeBridge>>;
