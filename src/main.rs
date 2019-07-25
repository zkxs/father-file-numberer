extern crate clap;
#[macro_use]
extern crate lazy_static;
extern crate regex;

use std::{cmp, convert::TryFrom, fs, io};
use std::path::Path;
use std::process::exit;

use clap::{App, Arg};
use regex::Regex;

// number of verbose flags that must be present for output to appear
const INFO_VERBOSITY: u32 = 1;

fn main() {
    let matches = App::new("father-file-numberer")
        .version("0.1.0")
        .author("Michael Ripley <zkxs00@gmail.com>")
        .about("Renumbers files")
        .arg(Arg::with_name("directory")
            .short("d")
            .long("directory")
            .takes_value(true)
            .value_name("DIRECTORY")
            .help("Sets directory to operate on. If not specified, uses the current working directory."))
        .arg(Arg::with_name("recursive")
            .short("r")
            .long("recursive")
            .help("enables directory recursion"))
        .arg(Arg::with_name("start")
            .short("S")
            .long("start")
            .takes_value(true)
            .value_name("START")
            .validator(is_numeric)
            .help("if present, will not match files with numbers lower than this"))
        .arg(Arg::with_name("end")
            .short("E")
            .long("end")
            .takes_value(true)
            .value_name("END")
            .validator(is_numeric)
            .help("if present, will not match files with numbers higher than this"))
        .arg(Arg::with_name("number_width")
            .short("w")
            .long("number-width")
            .takes_value(true)
            .value_name("NUMBER-WIDTH")
            .validator(is_number)
            .help("if present, will format output numbers to at least the specified width"))
        .arg(Arg::with_name("dry_run")
            .short("y")
            .long("dry-run")
            .help("do not operate, but print what would have been done"))
        .arg(Arg::with_name("verbose")
            .short("v")
            .long("verbose")
            .multiple(true)
            .help("increase verbosity"))
        .arg(Arg::with_name("offset")
            .required(true)
            .takes_value(true)
            .allow_hyphen_values(true)
            .value_name("OFFSET")
            .validator(is_numeric)
            .help("Number (positive or negative) to offset filenames by"))
        .get_matches();

    // parse arguments
    let recursive = matches.is_present("recursive");
    let start: Option<i32> = matches.value_of("start").map(|n| n.parse().unwrap());
    let end: Option<i32> = matches.value_of("end").map(|n| n.parse().unwrap());
    let directory = match matches.value_of("directory") {
        /* The path library is garbage and cannot both go above the top of
         * a relative path and also respect symlinks. Oh well, this is targeted
         * at Windows Dad so what are the chances he needs symlink support anyways...
         *
         * RIP symlinks
         */
        Some(path) => Path::new(path).canonicalize().unwrap(),
        None => Path::new(".").canonicalize().unwrap()
    };
    let number_width: Option<u32> = matches.value_of("number_width").map(|n| n.parse().unwrap());
    let dry_run = matches.is_present("dry_run");
    let verbosity = matches.occurrences_of("verbose") as u32;
    let offset: i32 = matches.value_of("offset").unwrap().parse().unwrap();

    // check directory
    if !directory.is_dir() {
        eprintln!("DIRECTORY is not a directory");
        exit(1);
    }

    if dry_run {
        println!("This is a dry run. No files will be renamed.");
    }

    // start recursion
    let adjuster = |x: i32| x + offset;
    process_directory(directory, recursive, start, end, dry_run, number_width, verbosity, &adjuster).unwrap();
}

fn process_directory<P: AsRef<Path>, F: Fn(i32) -> i32>(directory: P, recursive: bool, start: Option<i32>, end: Option<i32>, dry_run: bool, number_width: Option<u32>, verbosity: u32, adjuster: &F) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if recursive && path.is_dir() {
            match process_directory(path, recursive, start, end, dry_run, number_width, verbosity, adjuster) {
                Ok(()) => {}
                Err(e) => return Err(e)
            }
        } else {
            lazy_static! {
                static ref FILENAME: Regex = Regex::new(r#"^(.*?)([0-9]+)(.*?)$"#).unwrap();
            }
            let os_filename = entry.file_name(); // explicitly save this because it would get freed as a temporary
            let filename = os_filename.to_str().unwrap();
            match FILENAME.captures(filename) {
                Some(captures) => {
                    let prefix = captures.get(1).unwrap().as_str();
                    let number: i32 = captures.get(2).unwrap().as_str().parse().unwrap();
                    let suffix = captures.get(3).unwrap().as_str();

                    // check number range
                    let start_ok = start.map_or(true, |s| number >= s);
                    let end_ok = end.map_or(true, |e| number <= e);
                    let in_range = start_ok && end_ok;

                    if in_range {
                        let adjusted_number = adjuster(number);
                        let pad: usize = match number_width {
                            Some(width) => {
                                let needed_zeros: i32 = width as i32 - log10(u32::try_from(adjusted_number).unwrap()) as i32;
                                // make sure this isn't negative
                                usize::try_from(cmp::max(0, needed_zeros)).unwrap()
                            }
                            None => 0
                        };
                        let new_filename = format!("{}{}{}{}", prefix, "0".repeat(pad), adjusted_number, suffix);

                        let mut new_path = path.parent().unwrap().to_path_buf();
                        new_path.push(format!("{}", new_filename));
                        let path_str;
                        let new_path_str;
                        if recursive {
                            path_str = path.display().to_string();
                            new_path_str = new_path.display().to_string();
                        } else {
                            path_str = path.file_name().unwrap().to_string_lossy().into_owned();
                            new_path_str = new_path.file_name().unwrap().to_string_lossy().into_owned();
                        }

                        if dry_run {
                            println!("{} => {}", path_str, new_path_str)
                        } else {
                            match fs::rename(path.clone(), new_path.clone()) {
                                Ok(()) => println!("{} => {}", path_str, new_path_str),
                                Err(e) => eprintln!("ERROR {} => {}: {:?}", path_str, new_path_str, e)
                            }
                        }
                    } else {
                        if verbosity > INFO_VERBOSITY {
                            println!("skipping out of range file {:?}", filename)
                        }
                    }
                }
                None => {
                    if verbosity > INFO_VERBOSITY {
                        println!("skipping non matching file {:?}", filename)
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_numeric(v: String) -> Result<(), String> {
    lazy_static! {
        static ref NUMERIC: Regex = Regex::new(r#"^[\+\-]?[0-9]+$"#).unwrap();
    }
    if NUMERIC.is_match(&v) {
        Ok(())
    } else {
        Err(String::from("The value is not numeric"))
    }
}

fn is_number(v: String) -> Result<(), String> {
    lazy_static! {
        static ref NUMBER: Regex = Regex::new(r#"^[1-9][0-9]*$"#).unwrap();
    }
    if NUMBER.is_match(&v) {
        Ok(())
    } else {
        Err(String::from("The value is not numeric"))
    }
}

fn log2(n: u32) -> u32 {
    if n != 0 {
        32 - n.leading_zeros()
    } else {
        0
    }
}

fn log10(n: u32) -> u8 {
    static GUESS: [u8; 33] = [
        0, 0, 0, 0, 1, 1, 1, 2, 2, 2,
        3, 3, 3, 3, 4, 4, 4, 5, 5, 5,
        6, 6, 6, 6, 7, 7, 7, 8, 8, 8,
        9, 9, 9
    ];
    static TEN_TO_THE: [u32; 10] = [
        1, 10, 100, 1000, 10000, 100000,
        1000000, 10000000, 100000000, 1000000000
    ];
    let digits = GUESS[log2(n) as usize];
    let adjustment = if n >= TEN_TO_THE[digits as usize] {
        1
    } else {
        0
    };
    digits + adjustment
}
