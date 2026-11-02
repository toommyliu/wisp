import * as Schema from "effect/Schema";
import * as Rpc from "effect/rpc/Rpc";
import * as RpcGroup from "effect/rpc/RpcGroup";

import {
  GrabbedDataSchema,
  LoaderGrabberGrabRequestSchema,
  LoaderGrabberLoadRequestSchema,
} from "./loader-grabber";
import { PacketQueuePayloadSchema, PacketSendPayloadSchema } from "./packets";

export class LoaderGrabberError extends Schema.TaggedError<LoaderGrabberError>()(
  "LoaderGrabberError",
  {
    detail: Schema.String,
  },
) {
  override get message(): string {
    return this.detail;
  }
}

export class PacketsError extends Schema.TaggedError<PacketsError>()(
  "PacketsError",
  {
    detail: Schema.String,
  },
) {
  override get message(): string {
    return this.detail;
  }
}

export const LoaderGrabberRpcs = RpcGroup.make(
  Rpc.make("LoaderGrabberLoad", {
    payload: LoaderGrabberLoadRequestSchema,
    error: LoaderGrabberError,
  }),
  Rpc.make("LoaderGrabberGrab", {
    payload: LoaderGrabberGrabRequestSchema,
    success: Schema.NullOr(GrabbedDataSchema),
    error: LoaderGrabberError,
  }),
);

export const PacketsRpcs = RpcGroup.make(
  Rpc.make("PacketsStartCapture", { error: PacketsError }),
  Rpc.make("PacketsStopCapture", { error: PacketsError }),
  Rpc.make("PacketsSend", {
    payload: PacketSendPayloadSchema,
    error: PacketsError,
  }),
  Rpc.make("PacketsStartQueue", {
    payload: PacketQueuePayloadSchema,
    error: PacketsError,
  }),
  Rpc.make("PacketsStopQueue", { error: PacketsError }),
);

export const GameRendererRpcs = RpcGroup.make().merge(
  LoaderGrabberRpcs,
  PacketsRpcs,
);
