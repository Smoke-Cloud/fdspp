// Checks the built WASM package against the fdspp command: the same
// allocation for every example input. Run with `just test-wasm`.
import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { FdsParseError, loadFdspp } from "./pkg/fdspp.js";

const fdspp = await loadFdspp(readFileSync("wasm/pkg/fdspp.wasm"));

function inputs(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return inputs(path);
    return path.endsWith(".fds") ? [path] : [];
  });
}

let compared = 0;
let failures = 0;
for (const path of inputs("tests")) {
  for (const nMpi of [1, 3, 8]) {
    // The command writes its --json allocation to stderr.
    const run = spawnSync(
      "target/release/fdspp",
      [path, "--n-mpi", String(nMpi), "--json", "/dev/null"],
      { stdio: ["ignore", "ignore", "pipe"] },
    );
    const expected =
      run.status === 0
        ? JSON.parse(run.stderr.toString())
        : { error: run.stderr.toString() };
    let actual;
    try {
      const analysis = fdspp.analyse(readFileSync(path), nMpi);
      // Allocating the cells alone must agree with allocating the file.
      const fromCells = fdspp.allocate(analysis.meshCells, nMpi);
      if (JSON.stringify(fromCells) !== JSON.stringify(analysis.allocation)) {
        throw new Error(`allocate() disagrees with analyse() for ${path}`);
      }
      actual = { processes: analysis.allocation.processes };
    } catch (err) {
      if (!(err instanceof FdsParseError)) throw err;
      actual = { error: err.message };
    }
    compared++;
    // Both fail, or both give the same allocation.
    const same =
      "error" in expected
        ? "error" in actual
        : JSON.stringify(actual) === JSON.stringify(expected);
    if (!same) {
      failures++;
      console.error(`MISMATCH ${path} --n-mpi ${nMpi}`);
    }
  }
}
console.log(`${compared} allocations compared, ${failures} mismatched`);
process.exit(failures === 0 ? 0 : 1);
