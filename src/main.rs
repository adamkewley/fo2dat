extern crate fo2dat;

use std::error::Error;

const USAGE: &str = "fo2dat [OPTION...] [FILE]...";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.len() {
        2 => {
            let file_name = &args[1];
            if let Err(e) = fo2dat::run(file_name) {
                print_and_die(e.description(), 1);
            }
        }
        _ => {
            print_and_die(USAGE, 1);
        }
    }
}

fn print_and_die(s: &str, exit_code: i32) {
    println!("{}", s);
    std::process::exit(exit_code);
}
