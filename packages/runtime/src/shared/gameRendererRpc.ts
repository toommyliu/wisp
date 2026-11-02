import {
  FollowerConfigSchema,
  FollowerStateSchema,
} from "@lucent/schemas/follower";
import * as Schema from "effect/Schema";
import * as Rpc from "effect/rpc/Rpc";
import * as RpcGroup from "effect/rpc/RpcGroup";

export const FollowerRpcs = RpcGroup.make(
  Rpc.make("FollowerConfigure", {
    payload: FollowerConfigSchema,
    success: FollowerStateSchema,
  }),
  Rpc.make("FollowerGetState", { success: FollowerStateSchema }),
  Rpc.make("FollowerMe", { success: Schema.String }),
  Rpc.make("FollowerStart", {
    payload: FollowerConfigSchema,
    success: FollowerStateSchema,
  }),
  Rpc.make("FollowerStop", { success: FollowerStateSchema }),
);
