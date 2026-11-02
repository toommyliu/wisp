import { expect, it, vi } from "vitest";
import * as Effect from "effect/Effect";
import { createEmptyEnvironmentState } from "@lucent/schemas/environment";
import { makeScriptEnvironmentApi } from "../game/scripting/api/Environment";
import { makeEnvironmentService } from "./environment";
import { unavailableEnvironment } from "./services";

it("sends only the auto register fields supplied by the caller", async () => {
  const state = createEmptyEnvironmentState();
  const host = { read: vi.fn((_query: string) => JSON.stringify(state)), command: vi.fn((_request: string) => JSON.stringify(state)) };
  const service = makeEnvironmentService(host, unavailableEnvironment);
  await Effect.runPromise(makeScriptEnvironmentApi(service).updateQuestAutoRegister({ requirements: true }));
  await Effect.runPromise(service.setAutoRegisterRequirements(false));
  await Effect.runPromise(service.setAutoRegisterRewards(true));
  expect(host.command.mock.calls.map(([value]) => JSON.parse(value))).toEqual([
    { kind: "updateEnvironment", edit: { kind: "setQuestAutoRegister", requirements: true } },
    { kind: "updateEnvironment", edit: { kind: "setQuestAutoRegister", requirements: false } },
    { kind: "updateEnvironment", edit: { kind: "setQuestAutoRegister", rewards: true } },
  ]);
  expect(host.read).not.toHaveBeenCalled();
});
