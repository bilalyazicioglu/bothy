import test from "node:test";
import assert from "node:assert/strict";
import { version } from "../index.js";

test("the native module loads and reports the engine version", () => {
  assert.match(version(), /^\d+\.\d+\.\d+/);
});
