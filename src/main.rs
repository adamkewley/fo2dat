extern crate fo2dat;
extern crate clap;

use std::error::Error;
use clap::App;
use clap::Arg;
use std::env;

const APP_NAME: &str = "fo2dat";

fn main() {
    let matches = App::new(APP_NAME)
        .about("A Fallout 2 DAT archive utility")
        .arg(Arg::with_name("extract")
                 .short("x")
                 .long("extract")
                 .help("extract files from a DAT2 archive")
                 .takes_value(false))
        .arg(Arg::with_name("file")
            .short("f")
            .long("--file")
            .value_name("DAT2_FILE")
            .help("use file")
            .takes_value(true))
        .arg(Arg::with_name("list")
                 .short("t")
                 .long("list")
                 .help("list the contents of a DAT2 archive"))
        .arg(Arg::with_name("directory")
            .short("-C")
            .long("--directory")
            .help("change to dir before performing any operations")
            .takes_value(true))
        .get_matches();

    let maybe_file = matches.value_of("file");

    if let None = maybe_file {
        print_and_die("must provide file arg (-f)", 1);
    }

    let should_extract = matches.is_present("extract");
    let should_list = matches.is_present("list");

    if should_extract && should_list {
        print_and_die("you cannot specify more than one '-xt' option", 1);
    } else if should_list {
        if let Err(e) = fo2dat::list_contents(maybe_file.unwrap()) {
            print_and_die(e.description(), 1);
        }
    } else {
        let cwd = env::current_dir().unwrap();
        let output_path = matches.value_of("directory").unwrap_or(cwd.to_str().unwrap());
        if let Err(e) = fo2dat::extract(maybe_file.unwrap(), output_path) {
            print_and_die(e.description(), 1);
        }
    }
}

fn print_and_die(s: &str, exit_code: i32) {
    println!("{}: {}", APP_NAME, s);
    println!("Try '{} --help' for more information", APP_NAME);
    std::process::exit(exit_code);
}
