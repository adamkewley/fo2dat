extern crate fo2dat;
extern crate clap;
extern crate memmap;
extern crate flate2;
extern crate rayon;

use clap::App;
use clap::Arg;
use std::env;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::io::ErrorKind;
use memmap::Mmap;
use std::fs::File;
use flate2::read::ZlibDecoder;
use std::io::Error;
use std::io::Write;
use fo2dat::TreeEntry;
use rayon::prelude::*;


const APP_NAME: &str = "fo2dat";

enum CliAction {
    Extract,
    List,
}

struct CliArgs {
    action: CliAction,
    file: String,
    ch_dir: String,
    verbose: bool,
}

impl CliArgs {
    fn parse() -> io::Result<Self> {
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
            .arg(Arg::with_name("verbose")
                 .short("-v")
                 .long("--verbose")
                 .help("verbosely list files processed"))
            .get_matches();

        let should_extract = matches.is_present("extract");
        let should_list = matches.is_present("list");

        let action = if should_extract && should_list {
            Err(Error::new(ErrorKind::InvalidInput, "you cannot specify more than one '-xt' option"))
        } else if should_list {
            Ok(CliAction::List)
        } else if should_extract {
            Ok(CliAction::Extract)
        } else {
            Err(Error::new(ErrorKind::InvalidInput, "must specify either either '-t' or '-x'"))
        }?;

        let file = match matches.value_of("file").map(String::from) {
            Some(f) => Ok(f),
            None => Err(Error::new(ErrorKind::InvalidInput, "must provide file arg (-f)")),
        }?;

        let ch_dir = match matches.value_of("directory") {
            Some(dir) => String::from(dir),
            None => {
                let cwd = env::current_dir()?;
                String::from(cwd.to_str().unwrap())
            },
        };

        let verbose = matches.is_present("verbose");

        Ok(CliArgs { action, file, ch_dir, verbose })
    }
}

fn main() {
    match main_internal() {
        Ok(()) => {},
        Err(e) => {
            eprintln!("{}: {}", APP_NAME, e);
            eprintln!("Try '{} --help' for more information", APP_NAME);
            std::process::exit(1);
        },
    }
}

fn main_internal() -> io::Result<()> {
    let args = CliArgs::parse()?;

    match args.action {
        CliAction::Extract => extract_all_entries(&args),
        CliAction::List => list_entries(&args.file),
    }
}

/// Extract all entries in a DAT file located at `dat_path` to `output_dir`
fn extract_all_entries(args: &CliArgs) -> io::Result<()> {    
    let output_dir = PathBuf::from(&args.ch_dir);

    if !output_dir.exists() {
        let err_msg = format!("{}: no such directory", output_dir.to_str().unwrap());
        return Err(Error::new(ErrorKind::NotFound, err_msg));
    } else if !output_dir.is_dir() {
        let err_msg = format!("{}: not a directory", output_dir.to_str().unwrap());
        return Err(Error::new(ErrorKind::InvalidInput, err_msg));
    } else {
        extract_all_entries_to_dir(output_dir, mmap(&args.file)?, args)
    }
}

fn extract_all_entries_to_dir(output_dir: PathBuf, data: Mmap, args: &CliArgs) -> io::Result<()> {

    let tree_entries: io::Result<Vec<TreeEntry>> = fo2dat::iter_tree(&data)?.collect();
    let mut tree_entries = tree_entries?;
    tree_entries.sort_by(|e1, e2| e1.offset.cmp(&e2.offset));
    
    tree_entries.into_par_iter().try_for_each(|tree_entry| {
        let output_path = output_dir.join(&tree_entry.path);
        let entry_data = &data[tree_entry.offset..][..tree_entry.packed_size];

        write_entry(&entry_data, &output_path)?;

        if args.verbose {
            println!("{:?}", output_path);
        }

        Ok(())
    })
}

fn mmap(dat_path_str: &str) -> io::Result<Mmap> {
    let dat_path = Path::new(&dat_path_str);
    if dat_path.exists() {
        let dat_file = File::open(dat_path)?;
        unsafe { Mmap::map(&dat_file) }
    } else {
        let err_msg = format!("{}: no such file", dat_path_str);
        return Err(Error::new(ErrorKind::NotFound, err_msg));
    }
}

fn write_entry(entry_data: &[u8], output_path: &Path) -> io::Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut output_file = File::create(&output_path)?;

    if is_zlib_compressed(&entry_data) {
        let mut zlib_reader = ZlibDecoder::new(entry_data);
        std::io::copy(&mut zlib_reader, &mut output_file)?;
    } else {
        output_file.write(entry_data)?;
    }

    Ok(())
}

/// Returns true if `data` appears to be zlib compressed.
fn is_zlib_compressed(data: &[u8]) -> bool {
    const ZLIB_FIRST_MAGIC_BYTE: u8 = 0x78;
    const ZLIB_SECOND_MAGIC_BYTE: u8 = 0xda;

    data.len() > 2 && data[0] == ZLIB_FIRST_MAGIC_BYTE && data[1] == ZLIB_SECOND_MAGIC_BYTE
}

fn list_entries(dat_path: &str) -> io::Result<()> {
    let data = mmap(dat_path)?;

    for tree_entry in fo2dat::iter_tree(&data)? {
        let tree_entry = tree_entry?;
        println!("{}", tree_entry.path.to_str().unwrap());
    }

    Ok(())
}
