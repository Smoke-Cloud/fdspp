use std::io::{Read, Write};

use clap::{crate_name, crate_version, value_parser, Arg, Command};
use namelist::tokenizer::Token;

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
                .value_parser(value_parser!(u32))
                .num_args(1)
                .help("Number of MPI processes to use"),
        )
        .arg(
            Arg::new("OUTPUT-PATH")
                .required(true)
                .num_args(1)
                .help("Path to the input file or '-' for stdin"),
        )
        .get_matches();
    let mut input_handle: Box<dyn Read> = {
        let file_path = matches
            .get_one::<String>("INPUT-PATH")
            .expect("No input path");
        if file_path == "-" {
            Box::new(std::io::stdin())
        } else {
            Box::new(std::fs::File::open(file_path)?)
        }
    };
    let mut output_handle: Box<dyn Write> = {
        let file_path = matches
            .get_one::<String>("OUTPUT-PATH")
            .expect("No output path");
        if file_path == "-" {
            Box::new(std::io::stdout())
        } else {
            Box::new(std::fs::File::create(file_path)?)
        }
    };
    let input = {
        let mut buf = String::new();
        input_handle.read_to_string(&mut buf)?;
        buf
    };
    let parser = namelist::NmlParser::new(std::io::Cursor::new(input));
    let nmls: Vec<_> = parser.collect();
    let mut i = 0;
    for mut nml in nmls {
        if nml.tokens().get(1).map(|x| &x.token) == Some(&Token::Identifier("MESH".to_string())) {
            nml.append_token(Token::Identifier("MPI_PROCESS".to_string()));
            nml.append_token(Token::Equals);
            nml.append_token(Token::Number(format!("{i}")));
            i += 1;
        }
        output_handle.write_all(nml.to_string().as_bytes())?;
    }

    Ok(())
}
