import test from "node:test";
import assert from "node:assert/strict";
import { BothyError } from "../index.js";
import { env } from "./helpers.js";

async function pair(t) {
  const e = env();
  const a = await e.open("a");
  const b = await e.open("b");
  t.after(async () => {
    await Promise.all([a.close(), b.close()]);
    await e.cleanup();
  });
  const id = await a.initOrJoin();
  assert.equal(await b.initOrJoin(), id);
  return { a, b };
}

test("two devices converge through one folder", async (t) => {
  const { a, b } = await pair(t);
  const doc = {
    goal: "dock",
    n: 3,
    x: 1.5,
    big: 9007199254740993n,
    raw: new Uint8Array([1, 2, 3]),
    tags: ["a", "b"],
    o: { k: null },
  };
  await a.collection("missions").put("m1", doc);
  await a.collection("missions").put("m2", { goal: "scan" });

  const report = await a.sync();
  assert.equal(report.pushedOps, 2);
  assert.equal(report.passwordChanged, false);
  assert.deepStrictEqual(report.skipped, []);
  for (const key of ["pushedOps", "segmentsUp", "segmentsDown", "pulledOps", "changedDocs", "snapshotsDown", "snapshotsUp"]) {
    assert.equal(typeof report[key], "number", key);
  }

  const pulled = await b.sync();
  assert.equal(pulled.pulledOps, 2);
  assert.equal(pulled.changedDocs, 2);
  assert.deepStrictEqual(await b.collection("missions").get("m1"), doc);
  assert.deepStrictEqual(await b.collection("missions").get("m2"), { goal: "scan" });
});

test("edits to different fields merge, and both devices end up equal", async (t) => {
  const { a, b } = await pair(t);
  await a.collection("notes").put("n1", { title: "t", body: "b" });
  await a.sync();
  await b.sync();

  // Offline on both: each device replaces the document with one field changed.
  await a.collection("notes").put("n1", { title: "from a", body: "b" });
  await b.collection("notes").put("n1", { title: "t", body: "from b" });
  for (let i = 0; i < 2; i++) {
    await a.sync();
    await b.sync();
  }
  const fromA = await a.collection("notes").get("n1");
  const fromB = await b.collection("notes").get("n1");
  assert.deepStrictEqual(fromA, fromB);
  assert.deepStrictEqual(fromA, { title: "from a", body: "from b" });
});

test("after close(), every call rejects with BothyError and the other device still works", async (t) => {
  const { a, b } = await pair(t);
  const notes = a.collection("notes");
  await a.close();
  const bothy = (err) => err instanceof BothyError;
  await assert.rejects(a.sync(), bothy);
  await assert.rejects(notes.put("x", { a: 1 }), bothy);
  await assert.rejects(notes.get("x"), bothy);
  await assert.rejects(a.initOrJoin(), bothy);
  assert.equal(await b.collection("notes").put("y", { a: 1 }), true);
  await b.sync();
});
