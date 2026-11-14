//! fdspp for the browser: the same mesh parsing and MPI allocation the fdspp
//! command uses, so a web page can show how a model's meshes will be spread
//! across processes before it is submitted.
//!
//! The WASM interface is deliberately small and needs no bindings generator:
//! the caller copies its input into memory from `fdspp_alloc`, calls
//! `fdspp_analyse` (an input file) or `fdspp_allocate` (mesh cell counts), and
//! reads back a JSON result, which it then frees with `fdspp_free`. See
//! `pkg/fdspp.js` for the JavaScript side.

use fdspp::{FdsFile, MpiProcessAllocation};
use serde::{Deserialize, Serialize};

/// What fdspp makes of an input file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    /// The number of cells in each mesh, in the order they appear in the file.
    pub mesh_cells: Vec<usize>,
    /// How fdspp assigns the meshes to processes, if a number of processes
    /// was given: exactly what `fdspp --n-mpi` would do.
    pub allocation: Option<Allocation>,
}

/// How meshes are allocated to MPI processes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Allocation {
    /// Each process's meshes' cells, largest first, and their total.
    pub processes: Vec<MpiProcessAllocation>,
    /// The process each mesh is allocated to, in file order.
    pub mesh_process: Vec<usize>,
}

/// Why an input file couldn't be analysed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AnalysisError {
    pub message: String,
    /// Where in the file, counting from 1, if known.
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// Analyse an FDS input file. With `n_mpi` of 1 or more, also allocate its
/// meshes to that many MPI processes; with 0, only count the meshes' cells.
pub fn analyse(input: &str, n_mpi: u32) -> Result<Analysis, AnalysisError> {
    let to_error = |err: fdspp::FdsParseError| {
        let span = err.span();
        AnalysisError {
            message: err.to_string(),
            line: span.map(|s| s.line + 1),
            column: span.map(|s| s.column + 1),
        }
    };
    let fds_file = FdsFile::from_reader(input.as_bytes()).map_err(to_error)?;
    let mesh_cells = fds_file.mesh_cells().map_err(to_error)?;
    let allocation = (n_mpi > 0).then(|| allocate(&mesh_cells, n_mpi));
    Ok(Analysis {
        mesh_cells,
        allocation,
    })
}

/// Allocate meshes with these numbers of cells (in file order) to `n_mpi` MPI
/// processes, as fdspp would, without parsing the file again: e.g. to try
/// different numbers of processes.
///
/// # Panics
///
/// If `n_mpi` is 0.
pub fn allocate(mesh_cells: &[usize], n_mpi: u32) -> Allocation {
    let allocation = fdspp::allocate_cells(mesh_cells, n_mpi);
    Allocation {
        processes: allocation.outcome.processes,
        mesh_process: allocation.mesh_process,
    }
}

/// The JSON the WASM interface returns: `{"ok": T}` or `{"error": AnalysisError}`.
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Response<T> {
    Ok(T),
    Error(AnalysisError),
}

impl<T: Serialize> Response<T> {
    fn from_result(result: Result<T, AnalysisError>) -> Self {
        match result {
            Ok(value) => Self::Ok(value),
            Err(err) => Self::Error(err),
        }
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).expect("the response always serializes")
    }
}

fn plain_error(message: String) -> AnalysisError {
    AnalysisError {
        message,
        line: None,
        column: None,
    }
}

/// [`analyse`] an input file's bytes, as the WASM interface's JSON.
pub fn analyse_json(input: &[u8], n_mpi: u32) -> String {
    let result = std::str::from_utf8(input)
        .map_err(|err| plain_error(format!("the input file isn't valid UTF-8: {err}")))
        .and_then(|input| analyse(input, n_mpi));
    Response::from_result(result).to_json()
}

/// [`allocate`] the mesh cells in a JSON array, as the WASM interface's JSON.
pub fn allocate_json(mesh_cells: &[u8], n_mpi: u32) -> String {
    #[derive(Deserialize)]
    struct Cells(Vec<usize>);
    let result = if n_mpi == 0 {
        Err(plain_error(
            "at least one MPI process is needed".to_string(),
        ))
    } else {
        serde_json::from_slice::<Cells>(mesh_cells)
            .map_err(|err| plain_error(format!("invalid mesh cells: {err}")))
            .map(|Cells(cells)| allocate(&cells, n_mpi))
    };
    Response::from_result(result).to_json()
}

/// The WASM exports. Pointers and lengths are 32 bits on wasm32, which is what
/// lets [`fdspp_analyse`] return both in one u64.
#[cfg(target_arch = "wasm32")]
mod exports {
    /// Allocate `len` bytes for the caller to write the input file into.
    #[no_mangle]
    pub extern "C" fn fdspp_alloc(len: usize) -> *mut u8 {
        Box::into_raw(vec![0u8; len].into_boxed_slice()).cast()
    }

    /// Free memory from [`fdspp_alloc`] or a result from [`fdspp_analyse`].
    ///
    /// # Safety
    ///
    /// `ptr` and `len` must be exactly as returned, and freed only once.
    #[no_mangle]
    pub unsafe extern "C" fn fdspp_free(ptr: *mut u8, len: usize) {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }

    /// Analyse the `input_len` bytes of an input file at `input_ptr` (see
    /// [`super::analyse_json`]). Returns the JSON result; see [`pack`].
    ///
    /// # Safety
    ///
    /// `input_ptr` must point to `input_len` readable bytes.
    #[no_mangle]
    pub unsafe extern "C" fn fdspp_analyse(
        input_ptr: *const u8,
        input_len: usize,
        n_mpi: u32,
    ) -> u64 {
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        pack(super::analyse_json(input, n_mpi))
    }

    /// Allocate the mesh cells in the JSON array of `input_len` bytes at
    /// `input_ptr` (see [`super::allocate_json`]). Returns the JSON result;
    /// see [`pack`].
    ///
    /// # Safety
    ///
    /// `input_ptr` must point to `input_len` readable bytes.
    #[no_mangle]
    pub unsafe extern "C" fn fdspp_allocate(
        input_ptr: *const u8,
        input_len: usize,
        n_mpi: u32,
    ) -> u64 {
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        pack(super::allocate_json(input, n_mpi))
    }

    /// Hand a result to the caller: its pointer in the high 32 bits and its
    /// length in the low 32. The caller frees it with [`fdspp_free`].
    fn pack(result: String) -> u64 {
        let result = result.into_bytes().into_boxed_slice();
        let len = result.len();
        let ptr = Box::into_raw(result).cast::<u8>();
        ((ptr as u64) << 32) | len as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(name: &str) -> String {
        std::fs::read_to_string(format!("{}/../tests/{name}", env!("CARGO_MANIFEST_DIR")))
            .expect("test input")
    }

    #[test]
    fn counts_each_meshes_cells() {
        let input = "&HEAD CHID='t' /\n\
            &MESH IJK=10,10,10, XB=0,1,0,1,0,1 /\n\
            &MESH IJK=20,10,5, XB=1,2,0,1,0,1 /\n\
            &TAIL /\n";
        let analysis = analyse(input, 0).expect("test analysis");
        assert_eq!(analysis.mesh_cells, vec![1000, 1000]);
        assert_eq!(analysis.allocation, None);
    }

    #[test]
    fn allocates_like_the_command() {
        // The same allocation as the library the fdspp command uses.
        let input = read("room_fire.fds");
        let mut fds_file = FdsFile::from_reader(input.as_bytes()).expect("test parse");
        let expected = fds_file.allocate_mpi_processes(2).expect("test allocation");
        let analysis = analyse(&input, 2).expect("test analysis");
        let allocation = analysis.allocation.expect("allocation");
        assert_eq!(allocation.processes, expected.processes);
        assert_eq!(allocation.processes.len(), 2);
        let allocated: usize = allocation.processes.iter().map(|p| p.total).sum();
        assert_eq!(allocated, analysis.mesh_cells.iter().sum::<usize>());
    }

    #[test]
    fn reports_where_a_bad_file_is_wrong() {
        let err = analyse("&MESH XB=0,1,0,1,0,1 /\n", 1).expect_err("no IJK");
        assert!(err.message.contains("IJK"), "{err:?}");
        assert_eq!(err.line, Some(1));
    }

    #[test]
    fn json_responses() {
        let ok = analyse_json(b"&MESH IJK=2,2,2, XB=0,1,0,1,0,1 /\n", 1);
        assert_eq!(
            ok,
            r#"{"ok":{"meshCells":[8],"allocation":{"processes":[{"total":8,"meshes":[8]}],"meshProcess":[0]}}}"#
        );
        let bad = analyse_json(&[0xff, 0xfe], 1);
        assert!(bad.starts_with(r#"{"error":{"message":"the input file isn't valid UTF-8"#));
    }

    #[test]
    fn allocates_mesh_cells_without_the_file() {
        // Largest first, each to the emptiest process; ties in the order given.
        let allocation = allocate(&[100, 400, 100, 300, 100], 2);
        assert_eq!(allocation.mesh_process, vec![1, 0, 0, 1, 1]);
        let totals: Vec<usize> = allocation.processes.iter().map(|p| p.total).collect();
        assert_eq!(totals, vec![500, 500]);
        assert_eq!(
            allocate_json(b"[100,400,100,300,100]", 2),
            r#"{"ok":{"processes":[{"total":500,"meshes":[400,100]},{"total":500,"meshes":[300,100,100]}],"meshProcess":[1,0,0,1,1]}}"#
        );
        assert!(allocate_json(b"[1]", 0).starts_with(r#"{"error""#));
        assert!(allocate_json(b"nope", 1).starts_with(r#"{"error""#));
    }
}
