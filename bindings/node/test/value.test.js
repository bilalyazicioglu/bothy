import test, { before, after } from "node:test";
import vm from "node:vm";
import assert from "node:assert/strict";
import { BothyError } from "../index.js";
import { env } from "./helpers.js";

const e = env();
let db;
let notes;

before(async () => {
  db = await e.open("values");
  await db.initOrJoin();
  notes = db.collection("notes");
});

after(async () => {
  await db.close();
  await e.cleanup();
});

let n = 0;
/** Writes `doc` under a fresh id and reads it back. */
async function roundTrip(doc) {
  const id = `d${n++}`;
  await notes.put(id, doc);
  return notes.get(id);
}

const invalid = (err) => err instanceof BothyError && err.kind === "InvalidInput";

test("int and float values read back as numbers equal to the input", async () => {
  const got = await roundTrip({ a: 1, b: 1.5, c: 2 ** 53 - 1, d: -0.25, e: 0 });
  assert.deepStrictEqual(got, { a: 1, b: 1.5, c: 2 ** 53 - 1, d: -0.25, e: 0 });
});

test("integers above 2^53 come back as bigint, exactly", async () => {
  const got = await roundTrip({
    big: 9007199254740993n,
    min: -(2n ** 63n),
    max: 2n ** 63n - 1n,
    small: 5n,
  });
  assert.equal(typeof got.big, "bigint");
  assert.equal(got.big, 9007199254740993n);
  assert.equal(got.min, -(2n ** 63n));
  assert.equal(got.max, 2n ** 63n - 1n);
  assert.equal(got.small, 5, "a bigint in the safe range reads back as a number");
  await assert.rejects(notes.put("x", { n: 2n ** 63n }), invalid);
  await assert.rejects(notes.put("x", { n: -(2n ** 63n) - 1n }), invalid);
});

test("integer-valued numbers outside the safe range are rejected", async () => {
  for (const v of [2 ** 53, -(2 ** 53), 1e20, 1.5e300]) {
    await assert.rejects(notes.put("x", { n: v }), invalid, String(v));
  }
  assert.deepStrictEqual(await roundTrip({ n: 0.1 }), { n: 0.1 });
});

test("bytes round-trip as Uint8Array", async () => {
  const got = await roundTrip({
    b: new Uint8Array([0, 255, 1]),
    buf: Buffer.from([7, 8]),
    empty: new Uint8Array(0),
  });
  assert.ok(got.b instanceof Uint8Array);
  assert.deepStrictEqual([...got.b], [0, 255, 1]);
  assert.deepStrictEqual([...got.buf], [7, 8]);
  assert.equal(got.empty.length, 0);
  for (const bad of [new Int16Array(2), new Uint8ClampedArray(2), new Float64Array(1), new ArrayBuffer(2)]) {
    await assert.rejects(notes.put("x", { b: bad }), invalid, bad.constructor.name);
  }
});

test("values the format cannot hold are rejected", async () => {
  class A {}
  const bad = [
    ["NaN", NaN],
    ["Infinity", Infinity],
    ["-Infinity", -Infinity],
    ["undefined", undefined],
    ["Date", new Date()],
    ["Map", new Map()],
    ["Set", new Set()],
    ["class instance", new A()],
    ["Symbol", Symbol("x")],
    ["function", () => 1],
  ];
  for (const [name, value] of bad) {
    await assert.rejects(notes.put("x", { v: value }), invalid, name);
  }
  for (const doc of [[], 1, null, "text", new Uint8Array(1)]) {
    await assert.rejects(notes.put("x", doc), invalid, `document ${String(doc)}`);
  }
});

test("nested values keep their shape", async () => {
  const doc = {
    s: "héllo 😀",
    t: true,
    f: false,
    z: null,
    list: [1, "two", [3, [4]], { five: 5 }, null],
    obj: { a: { b: { c: [] } }, empty: {} },
  };
  assert.deepStrictEqual(await roundTrip(doc), doc);
});

test("a missing id gives null, a bad id is rejected", async () => {
  assert.equal(await notes.get("nope"), null);
  await assert.rejects(notes.put("", {}), invalid);
  await assert.rejects(notes.get(""), invalid);
});

test("put reports whether the document changed", async () => {
  assert.equal(await notes.put("same", { a: 1 }), true);
  assert.equal(await notes.put("same", { a: 1 }), false);
  assert.equal(await notes.put("same", { a: 2 }), true);
});

test("a __proto__ key in a nested object is an own data property, not a prototype change", async () => {
  const nested = JSON.parse('{"__proto__": {"x": 1}, "y": 2}');
  const got = await roundTrip({ nested });
  assert.ok(Object.hasOwn(got.nested, "__proto__"));
  assert.equal(Object.getPrototypeOf(got.nested), Object.prototype);
  assert.deepStrictEqual(got.nested.__proto__, { x: 1 });
  assert.equal(got.nested.y, 2);
  // Top-level field names starting with "_" belong to Bothy; the engine refuses them.
  await assert.rejects(notes.put("x", nested), invalid);
});

test("cyclic and too deeply nested values are rejected, not crashed on", async () => {
  const cyclic = { a: 1 };
  cyclic.self = cyclic;
  await assert.rejects(notes.put("x", cyclic), invalid);
  const cyclicArray = [];
  cyclicArray.push(cyclicArray);
  await assert.rejects(notes.put("x", { v: cyclicArray }), invalid);

  const nest = (depth) => {
    let v = [];
    for (let i = 1; i < depth; i++) v = [v];
    return v;
  };
  await assert.rejects(notes.put("x", { v: nest(40) }), invalid);
  // The engine allows nesting up to 32 levels in total, the document map being the first.
  assert.deepStrictEqual(await roundTrip({ v: nest(31) }), { v: nest(31) });
  await assert.rejects(notes.put("x", { v: nest(32) }), invalid);
});

test("a string with a lone surrogate is rejected, a surrogate pair is kept", async () => {
  await assert.rejects(notes.put("x", { s: "\ud800" }), invalid);
  await assert.rejects(notes.put("x", { s: "a\udc00b" }), invalid);
  await assert.rejects(notes.put("x", { ["\ud800"]: 1 }), invalid);
  assert.deepStrictEqual(await roundTrip({ s: "😀", [`k😀`]: "\0nul" }), { s: "😀", "k😀": "\0nul" });
});

test("arrays with holes are rejected", async () => {
  await assert.rejects(notes.put("x", { a: [1, , 3] }), invalid); // eslint-disable-line no-sparse-arrays
  await assert.rejects(notes.put("x", { a: new Array(3) }), invalid);
});

test("an object with a null prototype is a plain object", async () => {
  const doc = Object.assign(Object.create(null), { a: 1 });
  assert.deepStrictEqual(await roundTrip(doc), { a: 1 });
});

test("plain objects from another realm are accepted, other prototypes still are not", async () => {
  const doc = vm.runInNewContext("({ a: 1, nested: { b: [2, { c: 3 }] } })");
  assert.notEqual(Object.getPrototypeOf(doc), Object.prototype);
  assert.deepStrictEqual(await roundTrip(doc), { a: 1, nested: { b: [2, { c: 3 }] } });
  for (const [name, code] of [
    ["Date", "new Date()"],
    ["Map", "new Map()"],
    ["class instance", "new (class A {})()"],
  ]) {
    await assert.rejects(notes.put("x", { v: vm.runInNewContext(code) }), invalid, name);
  }
});

test("a caller's own exception is never turned into a BothyError, whatever its message says", async () => {
  const boom = new Error("bothy:Internal:not ours");
  const doc = {
    get x() {
      throw boom;
    },
  };
  await assert.rejects(notes.put("x", doc), (err) => err === boom);
});
