import type * as Protocol from "../../game/src/protocol";
import type { AssertEqual } from "../../game/src/protocol-contract";
import type { DeepReadonly } from "../../game/src/model";
import type {
  AccountScriptSettings,
  AccountScriptSettingsPatch,
  RoomPolicy,
} from "./accountSettings";

type Assert<T extends true> = T;
export type RoomPolicyProtocolContract = Assert<
  AssertEqual<RoomPolicy, DeepReadonly<Protocol.RoomPolicy>>
>;
export type ScriptOptionsProtocolContract = Assert<
  AssertEqual<AccountScriptSettings, DeepReadonly<Protocol.ScriptOptions>>
>;
export type ScriptOptionsPatchProtocolContract = Assert<
  AssertEqual<AccountScriptSettingsPatch, DeepReadonly<Protocol.ScriptOptionsPatch>>
>;
