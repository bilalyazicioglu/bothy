/** The version of the Bothy engine this package was built with. */
export function version(): string;

export type ErrorKind =
  | "Offline"
  | "AuthRequired"
  | "RateLimited"
  | "QuotaExceeded"
  | "RemoteInconsistent"
  | "FormatTooNew"
  | "SchemaTooNew"
  | "WrongPassword"
  | "KeyRequired"
  | "Corrupt"
  | "DatabaseLocked"
  | "InvalidInput"
  | "Internal";

/** Every error the package throws or rejects with. Messages never contain secrets. */
export class BothyError extends Error {
  readonly kind: ErrorKind;
  constructor(kind: ErrorKind, message: string);
}

/** A folder shared by the devices: a local directory, or one that another tool syncs. */
export class FolderRemote {
  constructor(path: string);
  readonly path: string;
}

export interface OpenOptions {
  /** Where the local database lives. */
  path: string;
  /** Your application's id, for example `com.example.notes`. */
  appId: string;
  remote: FolderRemote;
  deviceName?: string;
}

export class Database {
  static open(options: OpenOptions): Promise<Database>;
  /** Creates the database on the remote or joins the one that is there; returns its id (hex). */
  initOrJoin(): Promise<string>;
  /** Releases the local database. Calling it again is fine. */
  close(): Promise<void>;
}
