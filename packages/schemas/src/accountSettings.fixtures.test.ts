import { describe, expect, it } from "vitest";
import * as Schema from "effect/Schema";
import {
  AccountScriptSettingsPatchSchema,
  DEFAULT_ACCOUNT_SETTINGS,
  applyAccountSettingsPatch,
} from "./accountSettings";
import fixtures from "../test-fixtures/script-options.json";

const decode = Schema.decodeUnknownSync(AccountScriptSettingsPatchSchema);

describe("script options patches", () => {
  it.each(fixtures)("matches $name", ({ steps }) => {
    let state = DEFAULT_ACCOUNT_SETTINGS;
    for (const step of steps) {
      if (step.valid) {
        state = applyAccountSettingsPatch(state, { scripts: decode(step.patch) });
      } else {
        expect(() => decode(step.patch)).toThrow();
      }
      expect(state.scripts).toEqual(step.expected);
    }
  });
});
