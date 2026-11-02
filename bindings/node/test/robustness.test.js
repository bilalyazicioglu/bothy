import test from "node:test";
import assert from "node:assert/strict";
import { BothyError, Database, FolderRemote } from "../index.js";
import { APP, env } from "./helpers.js";

const invalid = (err) => err instanceof BothyError && err.kind === "InvalidInput";

async function setup(t) {
  const e = env();
  t.after(e.cleanup);
  const db = await e.open("r");
  await db.initOrJoin();
  return { e, db, notes: db.collection("notes") };
}

test("a Proxy cannot smuggle a Map, Date, array or typed array past the plain-object check", async (t) => {
  const { notes } = await setup(t);
  for (const [name, target] of [
    ["Map", new Map([[1, 2]])],
    ["Date", new Date()],
    ["array", [1, 2, 3]],
    ["Uint8Array", new Uint8Array(2)],
    ["class instance", new (class A { x = 1; })()],
  ]) {
    await assert.rejects(notes.put("x", { p: new Proxy(target, {}) }), invalid, name);
  }
  await notes.put("ok", { p: new Proxy({ a: 1 }, {}) });
  assert.deepStrictEqual(await notes.get("ok"), { p: { a: 1 } });
});

test("a lone surrogate in an id, a name or a path is rejected, so ids cannot collide", async (t) => {
  const { e, db, notes } = await setup(t);
  await assert.rejects(notes.put("id\ud800", { who: "first" }), invalid);
  await assert.rejects(notes.get("id\udfff"), invalid);
  assert.throws(() => db.collection("a\ud800"), invalid);
  assert.throws(() => new FolderRemote(`${e.dir}/r\ud800`), invalid);
  await assert.rejects(
    Database.open({ path: `${e.dir}/x\ud800.db`, appId: APP, remote: new FolderRemote(`${e.dir}/remote`) }),
    invalid,
  );
  await assert.rejects(
    Database.open({ path: e.path("n"), appId: APP, remote: new FolderRemote(`${e.dir}/remote`), deviceName: "d\ud800" }),
    invalid,
  );
});

test("arguments of the wrong type are InvalidInput", async (t) => {
  const { db, notes } = await setup(t);
  await assert.rejects(notes.put(1, {}), invalid);
  await assert.rejects(notes.get(undefined), invalid);
  assert.throws(() => db.collection(5), invalid);
  assert.throws(() => new FolderRemote(5), invalid);
  await assert.rejects(Database.open(), invalid);
  await assert.rejects(Database.open({}), invalid);
  await assert.rejects(Database.open({ path: 5, appId: APP, remote: new FolderRemote("/tmp") }), invalid);
});

test("an exception thrown by the caller's own getter comes back as that exception", async (t) => {
  const { notes } = await setup(t);
  const boom = new Error("boom");
  const doc = {
    get a() {
      throw boom;
    },
  };
  await assert.rejects(notes.put("x", doc), (err) => err === boom);
  assert.equal(await notes.put("y", { a: 1 }), true, "later calls still work");
});

test("close() waits for operations in flight, so the path can be reopened at once", async (t) => {
  const { e, db, notes } = await setup(t);
  const big = { items: Array.from({ length: 50_000 }, (_, i) => ({ i, s: `item ${i}` })) };
  const put = notes.put("big", big).then(() => "done", (err) => err);
  const sync = db.sync().then(() => "done", (err) => err);
  await db.close();
  const again = await Database.open({
    path: e.path("r"),
    appId: APP,
    remote: new FolderRemote(`${e.dir}/remote`),
  });
  await again.close();
  for (const result of await Promise.all([put, sync])) {
    assert.ok(result === "done" || result instanceof BothyError, String(result));
  }
});
