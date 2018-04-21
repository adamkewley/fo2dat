use std::path::Path;
use std::fs::File;
use std::io;
use std::error::Error;
use std::io::ErrorKind;
use std::io::BufReader;
use std::io::Read;

fn main() {
    let usage = "usage: fo2dat DAT_FILE";

    let args: Vec<String> = std::env::args().collect();

    match args.len() {
        2 => {
            if let Err(e) = run(&args[1]) {
                print_and_die(e.description(), 1);
            }
        }
        _ => {
            print_and_die(usage, 1);
        }
    }
}

fn print_and_die(s: &str, exit_code: i32) {
    println!("{}", s);
    std::process::exit(1);
}

fn run(dat_file_path: &String) -> io::Result<()> {
    let dat_file_path = Path::new(dat_file_path);
    let dat_file = File::open(dat_file_path)?;

    let reader = BufReader::new(dat_file);

    let bytes = reader.bytes()
        .filter(|result| result.is_ok())
        .map(|result| result.unwrap());

    print_hexdump(bytes);

    Ok(())
}

fn print_hexdump<I>(iter: I)
    where I: Iterator<Item = u8>
{
    iter.map(|byte| format!("{:02x}", byte))
        .enumerate()
        .take(50)
        .for_each(|(i, hex)| {
            if i % 2 == 0 {
                print!(" ")
            }

            if i % 5 == 0 {
                println!("{}", hex);
            } else {
                print!("{}", hex);
            }
        });
}