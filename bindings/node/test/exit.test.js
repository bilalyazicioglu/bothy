import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { env } from "./helpers.js";

const index = fileURLToPath(new URL("../index.js", import.meta.url));

test("the process exits by itself after close()", async (t) => {
  const e = env();
  t.after(e.cleanup);
  const script = `
    import { Database, FolderRemote } from ${JSON.stringify(index)};
    const db = await Database.open({
      path: ${JSON.stringify(e.path("exit"))},
      appId: "dev.bothy.nodetest",
      remote: new FolderRemote(${JSON.stringify(e.dir + "/remote")}),
    });
    await db.initOrJoin();
    await db.collection("c").put("d", { a: 1 });
    await db.sync();
    await db.close();
  `;
  const child = spawn(process.execPath, ["--input-type=module", "-e", script], { stdio: "pipe" });
  let stderr = "";
  child.stderr.on("data", (d) => (stderr += d));
  const timer = setTimeout(() => child.kill("SIGKILL"), 10_000);
  const [code, signal] = await new Promise((resolve) => child.on("exit", (c, s) => resolve([c, s])));
  clearTimeout(timer);
  assert.equal(signal, null, "the process had to be killed: something keeps the event loop alive");
  assert.equal(code, 0, stderr);
});
