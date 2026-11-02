import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Database, FolderRemote } from "../index.js";

export const APP = "dev.bothy.nodetest";

/** A temp directory with `open(name)` for databases that share one folder remote. */
export function env() {
  const dir = mkdtempSync(join(tmpdir(), "bothy-node-"));
  const remote = join(dir, "remote");
  return {
    dir,
    path: (name) => join(dir, `${name}.db`),
    open: (name) =>
      Database.open({
        path: join(dir, `${name}.db`),
        appId: APP,
        remote: new FolderRemote(remote),
        deviceName: name,
      }),
    cleanup: () => rmSync(dir, { recursive: true, force: true }),
  };
}
