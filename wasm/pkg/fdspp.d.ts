/** What fdspp makes of an input file. */
export interface Analysis {
  /** The number of cells in each mesh, in the order they appear in the file. */
  meshCells: number[];
  /**
   * How the meshes are allocated to MPI processes, exactly as
   * `fdspp --n-mpi` would allocate them. Null if no processes were asked for.
   */
  allocation: Allocation | null;
}

/** How meshes are allocated to MPI processes. */
export interface Allocation {
  /** Each process's meshes. */
  processes: ProcessAllocation[];
  /** The process each mesh is allocated to, in file order. */
  meshProcess: number[];
}

/** The meshes allocated to one MPI process. */
export interface ProcessAllocation {
  /** The process's total cells. */
  total: number;
  /** The cells in each of its meshes, largest first. */
  meshes: number[];
}

/** An input file fdspp couldn't make sense of. */
export declare class FdsParseError extends Error {
  /** Where in the file, counting from 1, if known. */
  line: number | undefined;
  column: number | undefined;
  constructor(
    message: string,
    line: number | undefined,
    column: number | undefined,
  );
}

/** A loaded fdspp module. */
export declare class Fdspp {
  private constructor();
  /**
   * Analyse an FDS input file: count each mesh's cells and, if `nMpi` is 1 or
   * more, allocate the meshes to that many MPI processes exactly as
   * `fdspp --n-mpi` would.
   *
   * @throws {FdsParseError} If the file can't be parsed.
   */
  analyse(input: string | Uint8Array, nMpi?: number): Analysis;
  /**
   * Allocate meshes with these numbers of cells (in file order, e.g. an
   * analysis's `meshCells`) to `nMpi` MPI processes, as fdspp would, without
   * parsing the file again: e.g. to try different numbers of processes.
   */
  allocate(meshCells: number[], nMpi: number): Allocation;
}

/**
 * Load fdspp's WASM module from `fdspp.wasm`: a URL to fetch (e.g. from a
 * bundler's `?url` import), a fetch Response, its bytes, or a compiled module.
 */
export declare function loadFdspp(
  source: string | URL | Response | BufferSource | WebAssembly.Module,
): Promise<Fdspp>;
