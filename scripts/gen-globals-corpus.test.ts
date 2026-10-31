import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("globals fixtures match Node", () => {
  execFileSync(process.execPath, [
    fileURLToPath(new URL("./gen-globals-corpus.mjs", import.meta.url)),
    "--check",
  ]);
});
