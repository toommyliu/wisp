import * as Cause from "effect/Cause";
import * as Effect from "effect/Effect";

import type { ApiService } from "../../flash/api/Api";
import type { Packet, PacketSelector } from "../../flash/contract/Packet";
import type { ScriptPacketApi, ScriptPacketOn } from "@lucent/scripting-api/contract";
import type { ScriptAsyncScope } from "../scriptAsyncScope";
import {
  notifyScriptCallbackFailure,
  type ScriptCallbackResult,
} from "./Callbacks";

export const makeScriptPacketApi = (
  packet: ApiService["packet"],
  scope: ScriptAsyncScope,
  failCause: (cause: Cause.Cause<unknown>) => Effect.Effect<void>,
): ScriptPacketApi => {
  const on = ((
    selector: PacketSelector | undefined,
    handler: (packet: Packet) => ScriptCallbackResult,
  ) =>
    packet
      .on(selector, notifyScriptCallbackFailure(handler, failCause))
      .pipe(
        Effect.tap((dispose) => scope.addCleanup(dispose)),
      )) as ScriptPacketOn;

  return Object.freeze({
    on,
    once: packet.once,
    sendToClient: packet.sendToClient,
    sendToServer: packet.sendToServer,
  });
};
