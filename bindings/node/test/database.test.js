import test from "node:test";
import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { BothyError, Database, FolderRemote } from "../index.js";
import { APP, env } from "./helpers.js";

const KINDS = [
  "Offline", "AuthRequired", "RateLimited", "QuotaExceeded", "RemoteInconsistent",
  "FormatTooNew", "SchemaTooNew", "WrongPassword", "KeyRequired", "Corrupt",
  "DatabaseLocked", "InvalidInput", "Internal",
];

test("open + initOrJoin returns a 32-char hex id and a second device joins the same id", async (t) => {
  const e = env();
  t.after(e.cleanup);
  const a = await e.open("a");
  const b = await e.open("b");
  t.after(() => Promise.all([a.close(), b.close()]));
  const id = await a.initOrJoin();
  assert.match(id, /^[0-9a-f]{32}$/);
  assert.equal(await b.initOrJoin(), id);
});

test("errors are BothyError with a kind", async (t) => {
  const e = env();
  t.after(e.cleanup);
  const file = join(e.dir, "not-a-dir");
  writeFileSync(file, "x");
  await assert.rejects(
    Database.open({ path: join(file, "x.db"), appId: APP, remote: new FolderRemote(join(e.dir, "r")) }),
    (err) => err instanceof BothyError && err instanceof Error && KINDS.includes(err.kind),
  );
  assert.throws(
    () => new FolderRemote(""),
    (err) => err instanceof BothyError && err.kind === "InvalidInput",
  );
  assert.throws(
    () => new FolderRemote(join(file, "remote")),
    (err) => err instanceof BothyError && KINDS.includes(err.kind),
  );
});

test("close is idempotent and frees the path", async (t) => {
  const e = env();
  t.after(e.cleanup);
  const a = await e.open("a");
  await a.close();
  await a.close();
  const again = await e.open("a");
  await again.close();
});

test("opening a path that is already open fails with DatabaseLocked", async (t) => {
  const e = env();
  t.after(e.cleanup);
  const a = await e.open("a");
  t.after(() => a.close());
  await assert.rejects(e.open("a"), (err) => err instanceof BothyError && err.kind === "DatabaseLocked");
});

test("initOrJoin after close rejects with BothyError", async (t) => {
  const e = env();
  t.after(e.cleanup);
  const a = await e.open("a");
  await a.close();
  await assert.rejects(a.initOrJoin(), (err) => err instanceof BothyError);
});
