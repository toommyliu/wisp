import type { SettingsPatch } from "@lucent/game/protocol";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";

import type { HostStateService } from "../state/HostState";

export const makeSettings = (host: HostStateService) => {
  const get = () => host.read({ kind: "settings" });
  return {
    apply: (patch: SettingsPatch) =>
      Effect.gen(function* () {
        for (const field of ["frameRate", "walkSpeed"] as const) {
          if (typeof patch[field] === "number" && !Number.isFinite(patch[field])) {
            return yield* Effect.die(new Error(`${field} must be a finite number`));
          }
        }
        const result = yield* host.command({ kind: "updateSettings", patch });
        if (Option.isNone(result)) {
          return yield* Effect.die(new Error("host rejected settings patch"));
        }
      }),
    get,
  };
};

export type Settings = ReturnType<typeof makeSettings>;
