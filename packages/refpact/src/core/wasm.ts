/**
 * The core of refpact, the Rust crate `refpact-core`, as WebAssembly (SPEC section 8).
 *
 * The module is `wasm/refpact_core.wasm` beside `src` and `dist`, built by `cargo xtask wasm`.
 * It is compiled once, on the first call, and speaks JSON requests. This is the only file of
 * the core that reads a file.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// The parts of the WebAssembly API that this file uses. Node has the whole API, but its types
// come with the DOM library, which this package does not compile with.
declare namespace WebAssembly {
  class Module {
    constructor(bytes: Uint8Array);
  }
  class Instance {
    constructor(module: Module, imports: object);
    readonly exports: object;
  }
  interface Memory {
    readonly buffer: ArrayBuffer;
  }
}

/** The version of the wire format that this package speaks. */
const VERSION = 1;

interface Exports {
  readonly memory: WebAssembly.Memory;
  refpact_alloc(len: number): number;
  refpact_call(address: number, len: number): number;
  refpact_answer(): number;
  refpact_answer_len(): number;
}

let core: Exports | undefined;
/** Counts the instances. A handle of a fact set is valid only in the instance that made it. */
let generation = 0;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function instance(): Exports {
  if (core !== undefined) {
    return core;
  }
  const url = new URL("../../wasm/refpact_core.wasm", import.meta.url);
  let bytes: Uint8Array;
  try {
    bytes = readFileSync(url);
  } catch (cause) {
    throw new Error(`refpact: the core is missing at ${fileURLToPath(url)}. Build it with \`cargo xtask wasm\`.`, { cause });
  }
  const made = new WebAssembly.Instance(new WebAssembly.Module(bytes), {}).exports as unknown as Exports;
  core = made;
  generation += 1;
  const { version } = JSON.parse(call('{"op":"version"}')) as { version: number };
  if (version !== VERSION) {
    core = undefined;
    throw new Error(`refpact: the core speaks version ${version} of the wire format, not ${VERSION}. Build it again with \`cargo xtask wasm\`.`);
  }
  return made;
}

/** The instance that the handles of fact sets belong to. */
export function currentGeneration(): number {
  instance();
  return generation;
}

/**
 * Sends one request and returns the answer as JSON text. Throws when the core finds the
 * request not valid, which is a bug of this package and not of the rule.
 */
export function call(request: string): string {
  const exports = instance();
  const bytes = encoder.encode(request);
  const address = exports.refpact_alloc(bytes.length);
  new Uint8Array(exports.memory.buffer, address, bytes.length).set(bytes);
  let status: number;
  try {
    status = exports.refpact_call(address, bytes.length);
  } catch (error) {
    // A trap leaves the instance in an unknown state. The next call starts a new one.
    core = undefined;
    throw error;
  }
  const answer = decoder.decode(new Uint8Array(exports.memory.buffer, exports.refpact_answer(), exports.refpact_answer_len()));
  if (status !== 0) {
    throw new Error(`refpact: the core rejected a request: ${answer}`);
  }
  return answer;
}
