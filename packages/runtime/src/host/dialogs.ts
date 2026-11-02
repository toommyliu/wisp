import type { ScriptDialogKind } from "@lucent/game/protocol";
import * as Effect from "effect/Effect";
import type { ScriptDialogs, ScriptDialogSource } from "../game/scripting/ScriptRuntime";
import { requestIo } from "./io";
import { HostServiceUnavailable } from "./services";

export const makeHostDialogs = (host: Pick<typeof __host, "requestIo" | "cancelIo">): ScriptDialogs => {
  const request = <A>(kind: ScriptDialogKind, message: string): Effect.Effect<A> =>
    Effect.tryPromise({
      try: async signal => {
        const result = await requestIo<{ ok: true; value: A } | { ok: false; error: string }>(
          host, { service: "dialog", kind, message: message.toWellFormed() }, signal);
        if (!result.ok) throw new Error(result.error);
        return result.value;
      },
      catch: error => error,
    }).pipe(Effect.orDie);
  return {
    beep: () => Effect.die(new HostServiceUnavailable("beep")),
    alert: (_source: ScriptDialogSource, message: string) => request<void>("Alert", message).pipe(Effect.asVoid),
    confirm: (_source: ScriptDialogSource, message: string) => request<boolean>("Confirm", message),
    prompt: (_source: ScriptDialogSource, message: string, placeholder = "") => request<string | null>({ Prompt: { placeholder: placeholder.toWellFormed() } }, message),
  };
};
