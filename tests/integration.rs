#[cfg(test)]
mod integration {
    #[test]
    fn parse_file_examples() {
        let example_dirs = std::fs::read_dir("tests/Examples").unwrap();
        for example_dir in example_dirs {
            let example_dir_path = example_dir.unwrap().path();
            if example_dir_path.is_dir() {
                let examples = std::fs::read_dir(&example_dir_path).unwrap();
                for example in examples {
                    let example_path = example.unwrap().path();
                    if example_path.is_file() {
                        eprintln!("{}",example_path.display());
                        let input = std::fs::read_to_string(&example_path).unwrap();
                        let parser = namelist::NmlParser::new(std::io::Cursor::new(&input));
                        let nmls: Vec<_> = parser.collect();
                        let mut new = String::new();
                        for nml in nmls {
                            new.push_str(&nml.to_string());
                        }
                        assert_eq!(input, new);
                    }
                }
            }
        }
    }
}
