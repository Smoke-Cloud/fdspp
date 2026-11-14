// fdspp in the browser: the same mesh parsing and MPI allocation as the fdspp
// command, compiled to WASM (see ../src/lib.rs for the other side of this
// interface).

/** An input file fdspp couldn't make sense of. */
export class FdsParseError extends Error {
  /**
   * @param {string} message
   * @param {number | undefined} line
   * @param {number | undefined} column
   */
  constructor(message, line, column) {
    super(message);
    this.name = "FdsParseError";
    /** Where in the file, counting from 1, if known. */
    this.line = line;
    this.column = column;
  }
}

/**
 * Load fdspp's WASM module.
 *
 * @param {string | URL | Response | BufferSource | WebAssembly.Module} source
 *   Where to get `fdspp.wasm`: a URL to fetch (e.g. from a bundler's `?url`
 *   import), a fetch Response, its bytes, or an already compiled module.
 * @returns {Promise<Fdspp>}
 */
export async function loadFdspp(source) {
  let module;
  if (source instanceof WebAssembly.Module) {
    module = source;
  } else if (source instanceof ArrayBuffer || ArrayBuffer.isView(source)) {
    module = await WebAssembly.compile(source);
  } else {
    const response = source instanceof Response ? source : await fetch(source);
    if (!response.ok) {
      throw new Error(
        `Failed to load fdspp.wasm: ${response.status} ${response.statusText}`,
      );
    }
    // Not compileStreaming, which fails unless the server sends the
    // application/wasm content type.
    module = await WebAssembly.compile(await response.arrayBuffer());
  }
  const instance = await WebAssembly.instantiate(module, {});
  return new Fdspp(instance.exports);
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** A loaded fdspp module. */
export class Fdspp {
  #exports;

  /** Use {@link loadFdspp} rather than constructing this directly. */
  constructor(exports) {
    this.#exports = exports;
  }

  /**
   * Analyse an FDS input file: count each mesh's cells and, if `nMpi` is 1 or
   * more, allocate the meshes to that many MPI processes exactly as
   * `fdspp --n-mpi` would.
   *
   * @param {string | Uint8Array} input The input file's contents.
   * @param {number} [nMpi] The number of MPI processes, or 0 for none.
   * @returns {import("./fdspp.d.ts").Analysis}
   * @throws {FdsParseError} If the file can't be parsed.
   */
  analyse(input, nMpi = 0) {
    checkProcesses(nMpi, 0);
    const bytes = typeof input === "string" ? encoder.encode(input) : input;
    return this.#call("fdspp_analyse", bytes, nMpi);
  }

  /**
   * Allocate meshes with these numbers of cells (in file order, e.g. an
   * analysis's `meshCells`) to `nMpi` MPI processes, as fdspp would, without
   * parsing the file again: e.g. to try different numbers of processes.
   *
   * @param {number[]} meshCells
   * @param {number} nMpi The number of MPI processes, at least 1.
   * @returns {import("./fdspp.d.ts").Allocation}
   */
  allocate(meshCells, nMpi) {
    checkProcesses(nMpi, 1);
    return this.#call(
      "fdspp_allocate",
      encoder.encode(JSON.stringify(meshCells)),
      nMpi,
    );
  }

  /** Call an export with `bytes` copied into WASM memory, and decode its JSON
   * result. */
  #call(name, bytes, nMpi) {
    const exports = this.#exports;
    const inputPtr = exports.fdspp_alloc(bytes.length);
    let packed;
    try {
      new Uint8Array(exports.memory.buffer, inputPtr, bytes.length).set(bytes);
      packed = exports[name](inputPtr, bytes.length, nMpi);
    } finally {
      exports.fdspp_free(inputPtr, bytes.length);
    }
    // The result's pointer is in the high 32 bits, its length in the low 32.
    const ptr = Number(packed >> 32n);
    const len = Number(packed & 0xffffffffn);
    // Read memory.buffer afresh: the call may have grown the memory.
    const json = decoder.decode(
      new Uint8Array(exports.memory.buffer, ptr, len),
    );
    exports.fdspp_free(ptr, len);
    const response = JSON.parse(json);
    if ("error" in response) {
      const { message, line, column } = response.error;
      throw new FdsParseError(message, line ?? undefined, column ?? undefined);
    }
    return response.ok;
  }
}

function checkProcesses(nMpi, min) {
  if (!Number.isInteger(nMpi) || nMpi < min || nMpi > 0xffffffff) {
    throw new RangeError(`Invalid number of MPI processes: ${nMpi}`);
  }
}
