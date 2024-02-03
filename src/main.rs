use clap::{
    crate_name, crate_version, value_parser, Arg, ArgAction, ArgGroup, ArgMatches, Command,
};
use fdspp::{FdsParseError, Transforms};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new(crate_name!())
        .arg_required_else_help(true)
        .version(crate_version!())
        .author("Jake O'Shannessy <joshannessy@smokecloud.io>")
        .about("FDS pre-processor")
        .arg(
            Arg::new("INPUT-PATH")
                .required(true)
                .num_args(1)
                .help("Path to the input file or '-' for stdin"),
        )
        .arg(
            Arg::new("N-PROCESSES")
                .long("n-mpi")
                .required(false)
                .value_parser(value_parser!(u64).range(1..))
                .num_args(1)
                .help("Number of MPI processes to use"),
        )
        .arg(
            Arg::new("IN-PLACE")
                .long("in-place")
                .short('i')
                .num_args(0)
                .action(ArgAction::SetTrue)
                .conflicts_with("OUTPUT-PATH")
                .help("Modify the input file in place"),
        )
        .arg(
            Arg::new("JSON")
                .long("json")
                .num_args(0)
                .action(ArgAction::SetTrue)
                .help("Log diagnostics to stderr using json"),
        )
        .arg(
            Arg::new("OUTPUT-PATH")
                .num_args(1)
                .help("Path to the input file or '-' for stdin"),
        )
        .group(
            ArgGroup::new("output")
                .args(["IN-PLACE", "OUTPUT-PATH"])
                .multiple(false)
                .required(true),
        )
        .get_matches();
    let input_path = matches
        .get_one::<String>("INPUT-PATH")
        .expect("No input path");
    let input_handle: Box<dyn Read> = {
        if input_path == "-" {
            if matches.get_flag("IN-PLACE") {
                panic!("cannot use --in-place while using stdin as an input")
            }
            Box::new(std::io::stdin())
        } else {
            Box::new(std::fs::File::open(input_path)?)
        }
    };
    let output_path = matches.get_one::<String>("OUTPUT-PATH");
    let output_handle: Option<Box<dyn Write>> = {
        if let Some(file_path) = output_path {
            if file_path == "-" {
                Some(Box::new(std::io::stdout()))
            } else {
                Some(Box::new(std::fs::File::create(file_path)?))
            }
        } else if matches.get_flag("IN-PLACE") {
            None
        } else {
            panic!("either an output path or in-place must be specified")
        }
    };
    let result = if let Some(output_handle) = output_handle {
        run(&matches, input_handle, output_handle)
    } else {
        let input_path = PathBuf::from(input_path);
        let input_dir = input_path.parent().unwrap();
        let mut output_file = tempfile::NamedTempFile::new_in(input_dir).unwrap();
        let result = run(&matches, input_handle, &mut output_file);
        if result.is_ok() {
            output_file.persist(input_path).unwrap();
        }
        result
    };
    match result {
        Ok(_) => Ok(()),
        Err(FdsParseError::Io(err)) => Err(Box::new(err)),
        Err(err) => {
            if let Some(span) = err.span() {
                eprintln!(
                    "ERROR: {}:{}:{} {err}",
                    input_path,
                    span.line + 1,
                    span.column + 1
                );
            } else {
                eprintln!("ERROR: {} {err}", input_path);
            }
            std::process::exit(1);
        }
    }
}

fn run(
    matches: &ArgMatches,
    input_handle: impl Read,
    output_handle: impl Write,
) -> Result<(), FdsParseError> {
    let transforms = Transforms {
        n_mpi: matches.get_one::<u64>("N-PROCESSES").copied(),
    };
    let outcomes = fdspp::apply_transforms(&transforms, input_handle, output_handle)?;
    if let Some(mesh_allocation) = outcomes.mesh_allocation {
        if matches.get_flag("JSON") {
            let s = serde_json::to_string_pretty(&mesh_allocation).unwrap();
            eprintln!("{s}");
        } else {
            let variation = {
                let process_cells: Vec<usize> =
                    mesh_allocation.processes.iter().map(|a| a.total).collect();
                let min_bucket = *process_cells.iter().min().unwrap() as f64;
                let max_bucket = *process_cells.iter().max().unwrap() as f64;
                ((max_bucket - min_bucket) / 2.0) / ((max_bucket + min_bucket) / 2.0) * 100.0
            };
            eprintln!("MPI Mesh Allocation");
            for (i, alloc) in mesh_allocation.processes.iter().enumerate() {
                eprintln!(
                    "  MPI_PROCESS {i}: TOTAL: {} {:?}",
                    alloc.total, alloc.meshes
                );
            }
            eprintln!("MPI Cell Count Variation: +/- {:.2}", variation);
        }
    }
    Ok(())
}
