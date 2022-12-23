use clap::{crate_name, crate_version, value_parser, Arg, ArgMatches, Command};
use namelist::{
    tokenizer::{NmlParseError, Span, Token, TokenizerError},
    Namelist, ParsedNamelist,
};
use std::{
    collections::HashMap,
    io::{Read, Write},
    num::ParseIntError,
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
            Arg::new("OUTPUT-PATH")
                .required(true)
                .num_args(1)
                .help("Path to the input file or '-' for stdin"),
        )
        .get_matches();
    let file_path = matches
        .get_one::<String>("INPUT-PATH")
        .expect("No input path");
    let input_handle: Box<dyn Read> = {
        if file_path == "-" {
            Box::new(std::io::stdin())
        } else {
            Box::new(std::fs::File::open(file_path)?)
        }
    };
    let output_handle: Box<dyn Write> = {
        let file_path = matches
            .get_one::<String>("OUTPUT-PATH")
            .expect("No output path");
        if file_path == "-" {
            Box::new(std::io::stdout())
        } else {
            Box::new(std::fs::File::create(file_path)?)
        }
    };
    match run(&matches, input_handle, output_handle) {
        Ok(_) => Ok(()),
        Err(FdsParseError::Io(err)) => Err(Box::new(err)),
        Err(err) => {
            if let Some(span) = err.span() {
                eprintln!(
                    "ERROR: {}:{}:{} {err}",
                    file_path,
                    span.line + 1,
                    span.column + 1
                );
            } else {
                eprintln!("ERROR: {} {err}", file_path);
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
    let mut fds_file = FdsFile::from_reader(input_handle)?;
    // If the number of MPI processes has been nominated, reallocate meshes.
    if let Some(n_mpi) = matches.get_one::<u64>("N-PROCESSES").copied() {
        fds_file.allocate_mpi_processes(n_mpi)?;
    }
    fds_file
        .write_all(output_handle)
        .map_err(FdsParseError::Io)?;
    Ok(())
}

#[derive(Debug)]
pub enum FdsParseError {
    Tokenize(TokenizerError),
    NmlParse(NmlParseError),
    Parse(Option<Span>, Box<dyn std::error::Error>),
    Io(std::io::Error),
}

impl FdsParseError {
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Tokenize(err) => Some(err.span()),
            Self::NmlParse(err) => err.span(),
            Self::Parse(span, _) => *span,
            Self::Io(_) => None,
        }
    }
}

impl std::fmt::Display for FdsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tokenize(err) => {
                write!(f, "{err}")
            }
            Self::NmlParse(err) => {
                write!(f, "{err}")
            }
            Self::Parse(_, err) => {
                write!(f, "{err}")
            }
            Self::Io(err) => {
                write!(f, "{err}")
            }
        }
    }
}

impl std::error::Error for FdsParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tokenize(err) => Some(err),
            Self::NmlParse(err) => Some(err),
            Self::Parse(_, _) => None,
            Self::Io(err) => Some(err),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FdsFile {
    pub nmls: Vec<Namelist>,
}

impl FdsFile {
    pub fn new(nmls: Vec<Namelist>) -> Self {
        Self { nmls }
    }

    pub fn from_reader(mut input_handle: impl Read) -> Result<Self, FdsParseError> {
        let input = {
            let mut buf = String::new();
            input_handle
                .read_to_string(&mut buf)
                .map_err(FdsParseError::Io)?;
            buf
        };
        let parser = namelist::NmlParser::new(std::io::Cursor::new(input));
        let nmls: Vec<_> = parser
            .collect::<Result<Vec<_>, TokenizerError>>()
            .map_err(FdsParseError::Tokenize)?;
        Ok(Self { nmls })
    }

    pub fn write_all(&self, mut output_handle: impl Write) -> std::io::Result<()> {
        for nml in self.nmls.iter() {
            output_handle.write_all(nml.to_string().as_bytes())?;
        }
        Ok(())
    }

    /// Count the number of cells in the model.
    pub fn n_cells(&self) -> Result<usize, FdsParseError> {
        let mut n = 0;
        for nml in self.nmls.iter() {
            if nml.tokens().get(1).map(|x| &x.token) == Some(&Token::Identifier("MESH".to_string()))
            {
                let pnml = ParsedNamelist::from_namelist(nml).map_err(FdsParseError::NmlParse)?;
                n += count_mesh_cells(&pnml)?;
            }
        }
        Ok(n)
    }

    pub fn allocate_mpi_processes(&mut self, n_mpi: u64) -> Result<(), FdsParseError> {
        let nmls = &mut self.nmls;
        let mut meshes = HashMap::new();
        {
            // Count cells
            for (i, nml) in nmls.iter().enumerate() {
                if nml.tokens().get(1).map(|x| &x.token)
                    == Some(&Token::Identifier("MESH".to_string()))
                {
                    let pnml =
                        ParsedNamelist::from_namelist(nml).map_err(FdsParseError::NmlParse)?;
                    let n_cells = count_mesh_cells(&pnml)?;
                    meshes.insert(i, n_cells);
                }
            }
        }
        let mut meshes: Vec<_> = meshes.into_iter().collect();
        meshes.sort_by(|a, b| a.1.cmp(&b.1));
        meshes.reverse();
        let mut buckets: Vec<Vec<(usize, usize)>> = vec![vec![]; n_mpi as usize];
        for mesh in &meshes {
            // get the minimum bucket
            let min_bucket = buckets
                .iter_mut()
                .min_by(|a, b| {
                    let n_a = a.iter().map(|(_, n)| n).sum::<usize>();
                    let n_b = b.iter().map(|(_, n)| n).sum::<usize>();
                    n_a.cmp(&n_b)
                })
                .unwrap();
            min_bucket.push(*mesh);
        }
        let mut mesh_process = HashMap::new();
        for (i, bucket) in buckets.iter().enumerate() {
            for (mesh_num, _) in bucket.iter() {
                mesh_process.insert(mesh_num, i);
            }
        }
        eprintln!("MPI Mesh Allocation");
        for (i, bucket) in buckets.iter().enumerate() {
            let total_cells = bucket.iter().map(|(_, n)| n).sum::<usize>();
            let cells: Vec<_> = bucket.iter().map(|(_, n)| n).collect();
            eprintln!("  MPI_PROCESS {i}: TOTAL: {total_cells} {cells:?}");
        }
        let process_cells: Vec<usize> = buckets
            .iter()
            .map(|buckets| buckets.iter().map(|(_, n)| n).sum::<usize>())
            .collect();
        let min_bucket = *process_cells.iter().min().unwrap() as f64;
        let max_bucket = *process_cells.iter().max().unwrap() as f64;
        let variation =
            ((max_bucket - min_bucket) / 2.0) / ((max_bucket + min_bucket) / 2.0) * 100.0;
        eprintln!("MPI Cell Count Variation: +/- {:.2}", variation);
        let mut old_mesh_locations: Vec<usize> = meshes.iter().map(|(i, _)| *i).collect();
        old_mesh_locations.sort();
        for (i, nml) in nmls.iter_mut().enumerate() {
            if nml.tokens().get(1).map(|x| &x.token) == Some(&Token::Identifier("MESH".to_string()))
            {
                let process_num = mesh_process.get(&i).unwrap();
                nml.append_token(Token::Identifier("MPI_PROCESS".to_string()));
                nml.append_token(Token::Equals);
                nml.append_token(Token::Number(format!("{process_num}")));
                nml.append_token(Token::Whitespace(" ".to_string()));
            }
        }
        let mut new_meshes = vec![];
        for &old_location in old_mesh_locations.iter() {
            let mut mesh: Namelist = Namelist::Other { tokens: vec![] };
            std::mem::swap(&mut mesh, nmls.get_mut(old_location).unwrap());
            new_meshes.push((mesh_process.get(&old_location).unwrap(), mesh));
        }
        new_meshes.sort_by(|a, b| a.0.cmp(b.0));
        for (old_location, (_, mut nml)) in old_mesh_locations.iter().zip(new_meshes.into_iter()) {
            std::mem::swap(&mut nml, nmls.get_mut(*old_location).unwrap());
        }
        Ok(())
    }
}

fn count_mesh_cells(pnml: &ParsedNamelist) -> Result<usize, FdsParseError> {
    let ijk = pnml.parameters.get("IJK").ok_or(FdsParseError::Parse(
        pnml.span,
        "no IJK parameter for mesh".into(),
    ))?;
    let values: Vec<usize> = ijk
        .values
        .iter()
        .map(|v| match v.token {
            Token::Number(ref s) => Ok(s
                .parse()
                .map_err(|err: ParseIntError| FdsParseError::Parse(v.span(), err.into()))?),
            _ => Err(FdsParseError::Parse(
                v.span(),
                "ERROR: invalid token for IJK".into(),
            )),
        })
        .collect::<Result<Vec<usize>, _>>()?;
    let ijk: [usize; 3] = values.try_into().map_err(|_| {
        FdsParseError::Parse(pnml.span, "incorrect number of IJK parameters".into())
    })?;
    Ok(ijk[0] * ijk[1] * ijk[2])
}
