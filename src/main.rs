use std::{
    collections::HashMap,
    io::{Read, Write},
    slice::Iter,
};

use clap::{crate_name, crate_version, value_parser, Arg, Command};
use namelist::{
    tokenizer::{LocatedToken, Token},
    Namelist,
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
                .value_parser(value_parser!(usize))
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
    let mut nmls: Vec<_> = parser.collect();
    if let Some(n_mpi) = matches.get_one::<usize>("N-PROCESSES").copied() {
        allocate_mpi_processes(&mut nmls, n_mpi);
    }
    for nml in nmls.into_iter() {
        output_handle.write_all(nml.to_string().as_bytes())?;
    }

    Ok(())
}

fn allocate_mpi_processes(nmls: &mut [Namelist], n_mpi: usize) {
    let mut meshes = HashMap::new();
    {
        // Count cells
        for (i, nml) in nmls.iter().enumerate() {
            if nml.tokens().get(1).map(|x| &x.token) == Some(&Token::Identifier("MESH".to_string()))
            {
                let pnml = parse_namelist(nml).unwrap();
                let n_cells = count_mesh_cells(&pnml).unwrap();
                meshes.insert(i, n_cells);
            }
        }
    }
    let mut meshes: Vec<_> = meshes.into_iter().collect();
    meshes.sort_by(|a, b| a.1.cmp(&b.1));
    meshes.reverse();
    let mut buckets: Vec<Vec<(usize, usize)>> = vec![vec![]; n_mpi];
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
    let variation = ((max_bucket - min_bucket) / 2.0) / ((max_bucket + min_bucket) / 2.0) * 100.0;
    eprintln!("MPI Cell Count Variation: +/- {:.2}", variation);
    let mut old_mesh_locations: Vec<usize> = meshes.iter().map(|(i, _)| *i).collect();
    old_mesh_locations.sort();
    for (i, nml) in nmls.iter_mut().enumerate() {
        if nml.tokens().get(1).map(|x| &x.token) == Some(&Token::Identifier("MESH".to_string())) {
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
    // TODO: Run a very inefficient selection sort.
    for (old_location, (_, mut nml)) in old_mesh_locations.iter().zip(new_meshes.into_iter()) {
        std::mem::swap(&mut nml, nmls.get_mut(*old_location).unwrap());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedNamelist {
    pub group: String,
    pub parameters: HashMap<String, ParameterValues>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterValues {
    pub dimensions: Vec<Token>,
    pub values: Vec<Token>,
}

fn next_non_ws<'a>(tokens: &'a mut Iter<LocatedToken>) -> Option<&'a LocatedToken> {
    loop {
        let token = tokens.next()?;
        match token.token {
            Token::Whitespace(_) | Token::Comma => {
                continue;
            }
            _ => return Some(token),
        }
    }
}

fn parse_namelist(nml: &namelist::Namelist) -> Option<ParsedNamelist> {
    let mut tokens = nml.tokens().iter();
    if tokens.next()?.token != Token::Ampersand {
        return None;
    }
    let group = if let Token::Identifier(s) = &tokens.next()?.token {
        s.to_string()
    } else {
        return None;
    };
    let mut parameters: HashMap<String, ParameterValues> = Default::default();
    let mut token_buf: Vec<Token> = vec![];
    while let Some(pn) = token_buf
        .pop()
        .as_ref()
        .or_else(|| next_non_ws(&mut tokens).map(|lt| &lt.token))
    {
        // Take parameter name
        let parameter_name = pn.clone();
        if parameter_name == Token::RightSlash {
            break;
        }
        if let Token::Identifier(name) = &parameter_name {
            {
                let b = token_buf.pop();
                let token = b
                    .as_ref()
                    .or_else(|| next_non_ws(&mut tokens).map(|lt| &lt.token));
                if let Some(Token::Equals) = token {
                } else {
                    panic!("no equals: {token:?}");
                };
            }
            let mut value_tokens: Vec<Token> = vec![];
            while let Some(token) = token_buf
                .pop()
                .as_ref()
                .or_else(|| next_non_ws(&mut tokens).map(|lt| &lt.token))
            {
                if token == &Token::RightSlash {
                    break;
                }
                if token == &Token::Equals {
                    token_buf.push(token.clone());
                    if let Some(t) = value_tokens.pop() {
                        token_buf.push(t);
                    }
                    break;
                }
                value_tokens.push(token.clone());
            }
            parameters.insert(
                name.to_string(),
                ParameterValues {
                    dimensions: vec![],
                    values: value_tokens,
                },
            );
            // loop until we hit equals or right slash
            continue;
        } else {
            panic!("invalid parameter name {:?}", parameter_name);
        }
    }
    Some(ParsedNamelist { group, parameters })
}

fn count_mesh_cells(pnml: &ParsedNamelist) -> Option<usize> {
    let ijk = pnml.parameters.get("IJK")?;
    let values: Vec<usize> = ijk
        .values
        .iter()
        .map(|v| match v {
            Token::Number(ref s) => s.parse().unwrap(),
            _ => panic!("invalid number"),
        })
        .collect();
    let ijk: [usize; 3] = values.try_into().unwrap();
    Some(ijk[0] * ijk[1] * ijk[2])
}
// Identifier("IJK") }
// LocatedToken { span: Span { lo: 43481, len: 1 }, token: Equals }
// LocatedToken { span: Span { lo: 43482, len: 2 }, token: Number("36") }
// LocatedToken { span: Span { lo: 43484, len: 1 }, token: Comma }
// LocatedToken { span: Span { lo: 43485, len: 2 }, token: Number("52") }
// LocatedToken { span: Span { lo: 43487, len: 1 }, token: Comma }
// LocatedToken { span: Span { lo: 43488, len: 1 }, token: Number("8") }
// LocatedToken { span: Span { lo: 43489, len: 1 }, token: Whitespace(" ") }
