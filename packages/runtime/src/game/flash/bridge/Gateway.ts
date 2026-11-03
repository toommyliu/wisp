import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as PubSub from "effect/PubSub";
import * as Queue from "effect/Queue";
import * as Schema from "effect/Schema";
import * as Stream from "effect/Stream";

import { decodeCallback, type Callback } from "../contract/Callback";
import type { DiagnosticPhase } from "../contract/Diagnostic";
import type { Event, ProjectionEvent, RuntimeEvent } from "../contract/Event";
import type {
  Packet,
  PacketDirection,
  PacketEncoding,
  RawPacket,
} from "../contract/Packet";
import {
  isUnsupportedPacketEnvelope,
  parsePacket,
} from "../protocol/PacketCodec";
import type { BridgeService } from "./Bridge";
import { DiagnosticSink } from "./DiagnosticSink";

type CallbackKey =
  | "onConnection"
  | "onDebug"
  | "onExtensionResponse"
  | "onProjectionEvent"
  | "packetFromClient"
  | "packetFromServer";

type Delivery =
  | Callback
  | { readonly type: "projection"; readonly event: ProjectionEvent };

type OwnedCallbacks = Required<Pick<Window, CallbackKey>>;

export type ClientPacketEncoding = PacketEncoding | "xml";
export type ServerPacketEncoding = PacketEncoding;

const clientPacketTypes = {
  json: "json",
  string: "str",
  xml: "xml",
} as const;

const serverPacketTypes = {
  json: "Json",
  string: "String",
} as const;

const packetInput = (
  callback: Delivery,
):
  | { readonly direction: PacketDirection; readonly raw: string }
  | undefined => {
  switch (callback.type) {
    case "client-packet":
      return { direction: "client", raw: callback.raw };
    case "extension-packet":
      return { direction: "extension", raw: callback.raw };
    case "server-packet":
      return { direction: "server", raw: callback.raw };
    default:
      return undefined;
  }
};

const runtimeEvent = (callback: Delivery): RuntimeEvent | undefined => {
  switch (callback.type) {
    case "connection":
      return { type: "connection", status: callback.status };
    case "debug":
      return { type: "debug", message: callback.message };
    default:
      return undefined;
  }
};

export const makeGateway = (bridge: BridgeService, target?: Window) =>
  Effect.gen(function* () {
    const resolvedTarget = target ?? window;
    const diagnosticSink = yield* DiagnosticSink;
    const scope = yield* Effect.scope;
    const callbacks = yield* Queue.unbounded<Delivery>();
    const events = yield* PubSub.unbounded<Event>();
    const packets = yield* PubSub.unbounded<Packet>();
    const rawPackets = yield* PubSub.unbounded<RawPacket>();
    let started = false;

    yield* Effect.addFinalizer(() =>
      Effect.all(
        [
          Queue.shutdown(callbacks),
          PubSub.shutdown(events),
          PubSub.shutdown(packets),
          PubSub.shutdown(rawPackets),
        ],
        { discard: true },
      ),
    );

    const reportDiagnosticUnsafe = (
      phase: DiagnosticPhase,
      operation: string,
      cause: unknown,
      args?: readonly unknown[],
    ): void => {
      diagnosticSink.report(phase, operation, cause, args);
    };

    const enqueue = (operation: string, delivery: Delivery): void => {
      if (!Queue.offerUnsafe(callbacks, delivery)) {
        reportDiagnosticUnsafe(
          "callback-dispatch",
          operation,
          new Error("Callback queue is unavailable"),
        );
      }
    };

    const offer = (operation: string, value: unknown): void => {
      const decoded = decodeCallback(value);
      if (Option.isNone(decoded)) {
        reportDiagnosticUnsafe(
          "callback-decode",
          operation,
          new Error("Invalid callback input"),
          [value],
        );
        return;
      }
      enqueue(operation, decoded.value);
    };

    const owned: OwnedCallbacks = {
      onConnection: (status) =>
        offer("onConnection", { type: "connection", status }),
      onDebug: (message) => offer("onDebug", { type: "debug", message }),
      onExtensionResponse: (raw) =>
        offer("onExtensionResponse", { type: "extension-packet", raw }),
      onProjectionEvent: (event) =>
        enqueue("onProjectionEvent", { type: "projection", event }),
      packetFromClient: (raw) =>
        offer("packetFromClient", { type: "client-packet", raw }),
      packetFromServer: (raw) =>
        offer("packetFromServer", { type: "server-packet", raw }),
    };

    for (const key of Object.keys(owned) as CallbackKey[]) {
      resolvedTarget[key] = owned[key] as never;
    }

    yield* Effect.addFinalizer(() =>
      Effect.sync(() => {
        for (const key of Object.keys(owned) as CallbackKey[]) {
          if (resolvedTarget[key] === owned[key]) delete resolvedTarget[key];
        }
      }),
    );

    const dispatch = (
      delivery: Delivery,
      handle: (event: Event) => Effect.Effect<void>,
    ) =>
      Effect.gen(function* () {
        const event =
          delivery.type === "projection"
            ? delivery.event
            : runtimeEvent(delivery);
        if (event !== undefined) {
          yield* handle(event);
          yield* PubSub.publish(events, event);
          return;
        }

        const input = packetInput(delivery);
        if (input === undefined) return;

        yield* PubSub.publish(rawPackets, input);
        const packet = parsePacket(input.direction, input.raw);
        if (Option.isNone(packet)) {
          if (isUnsupportedPacketEnvelope(input.raw)) return;
          yield* Effect.sync(() =>
            diagnosticSink.report(
              "packet-decode",
              `${input.direction}:packet`,
              new Error("Malformed packet envelope"),
              [input.raw],
            ),
          );
          return;
        }

        yield* PubSub.publish(packets, packet.value);
      });

    const start = (
      handle: (event: Event) => Effect.Effect<void> = () => Effect.void,
    ) =>
      Effect.suspend(() => {
        if (started) return Effect.void;
        started = true;
        const drain = Effect.forever(
          Queue.take(callbacks).pipe(
            Effect.flatMap((delivery) => dispatch(delivery, handle)),
            Effect.catchCause((cause) =>
              Effect.sync(() =>
                diagnosticSink.report("callback-dispatch", "drain", cause),
              ),
            ),
          ),
        );
        return Effect.forkIn(drain, scope).pipe(Effect.asVoid);
      });

    const sendToClient = (
      packet: string,
      encoding: ClientPacketEncoding = "string",
    ) =>
      bridge
        .invoke(
          "flash.sendClientPacket",
          [packet, clientPacketTypes[encoding]],
          Schema.Void,
        )
        .pipe(Effect.map(Option.isSome));

    const sendToServer = (
      packet: string,
      encoding: ServerPacketEncoding = "string",
    ) =>
      bridge
        .invoke(
          "flash.callGameFunction",
          [`sfc.send${serverPacketTypes[encoding]}`, packet],
          Schema.String,
        )
        .pipe(Effect.map(Option.isSome));

    return {
      events: Stream.fromPubSub(events),
      packets: Stream.fromPubSub(packets),
      rawPackets: Stream.fromPubSub(rawPackets),
      sendToClient,
      sendToServer,
      start,
      subscribeEvents: PubSub.subscribe(events),
      subscribePackets: PubSub.subscribe(packets),
    };
  });

export type GatewayService = Effect.Success<ReturnType<typeof makeGateway>>;
