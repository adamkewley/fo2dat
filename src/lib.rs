extern crate memmap;
extern crate byteorder;
extern crate flate2;

use std::io;
use std::path::Path;
use std::fs::File;
use memmap::Mmap;
use byteorder::{LittleEndian, ByteOrder};
use std::str;
use std::path::PathBuf;
use std::io::Write;
use flate2::read::ZlibDecoder;
use std::io::Error;
use std::io::ErrorKind;

const ZLIB_FIRST_MAGIC_BYTE: u8 = 0x78;
const ZLIB_SECOND_MAGIC_BYTE: u8 = 0xda;


const DAT_FILE_FOOTER_BYTES: usize = 8;
const DAT_FILE_MIN_SIZE: usize = DAT_FILE_FOOTER_BYTES + DATA_SECTION_TERMINATOR_LEN;

const DATA_SECTION_TERMINATOR_LEN: usize = 1;

const TREE_ENTRY_HEADER_SIZE: usize = 4;
const TREE_ENTRY_FOOTER_SIZE: usize = 13;
const TREE_ENTRY_MIN_SIZE: usize = TREE_ENTRY_HEADER_SIZE + TREE_ENTRY_FOOTER_SIZE;
const TREE_ENTRY_PATH_SEPARATOR: char = '\\';

#[derive(Debug)]
pub struct TreeEntry {
    filename: PathBuf,
    is_compressed: bool,
    decompressed_size: usize,
    packed_size: usize,
    offset: usize,
}

pub struct TreeEntryIterator<'a> {
    data: &'a [u8],
    offset: usize,
}

impl <'a> Iterator for TreeEntryIterator<'a> {
    type Item = io::Result<TreeEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.data.len() {
            return None
        }

        match  read_tree_entry(&self.data[self.offset..]) {
            Ok((entry, entry_size)) => {
                self.offset += entry_size;
                Some(Ok(entry))
            },
            Err(e) => {
                self.offset = std::usize::MAX; // makes next iter ret None
                Some(Err(e))
            }
        }
    }
}

pub fn read_tree_entries(data: &[u8]) -> TreeEntryIterator {
    TreeEntryIterator {
        data,
        offset: 0,
    }
}

pub fn read_tree_entry(data: &[u8]) -> io::Result<(TreeEntry, usize)> {
    if data.len() < TREE_ENTRY_MIN_SIZE {
        let err_msg = format!("remaining tree data is too small to actually fit a tree entry");
        return Err(Error::new(ErrorKind::InvalidData, err_msg));
    }

    let filename_len =
        LittleEndian::read_u32(&data[0..TREE_ENTRY_HEADER_SIZE]) as usize;

    let total_len = TREE_ENTRY_HEADER_SIZE + filename_len + TREE_ENTRY_FOOTER_SIZE;

    if data.len() < total_len {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("not enough space remaining in tree data to accommodate a filename + relevant footers");
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }

    let filename = match  str::from_utf8(&data[TREE_ENTRY_HEADER_SIZE..TREE_ENTRY_HEADER_SIZE+filename_len]) {
        Ok(s) => {
            let mut filename = PathBuf::new();
            for el in s.split(TREE_ENTRY_PATH_SEPARATOR) {
                filename.push(el);
            }
            Ok(filename)
        },
        Err(_) => {
            let err_msg = format!("cannot decode filename as ASCII");
            Err(Error::new(ErrorKind::InvalidData, err_msg))
        }
    }?;

    let footer_start = TREE_ENTRY_HEADER_SIZE + filename_len;
    let footer_end = footer_start + TREE_ENTRY_FOOTER_SIZE;
    let footer_data = &data[footer_start..footer_end];

    let tree_entry = TreeEntry {
        filename,
        is_compressed: data[0] > 0,
        decompressed_size: LittleEndian::read_u32(&footer_data[1..5]) as usize,
        packed_size: LittleEndian::read_u32(&footer_data[5..9]) as usize,
        offset: LittleEndian::read_u32(&footer_data[9..13]) as usize,

    };

    Ok((tree_entry, total_len))
}

pub fn list_contents(dat_path_str: &str) -> io::Result<()> {
    let dat_file = mmap_dat(&dat_path_str)?;
    let tree_entries = find_entries(&dat_file)?;

    for tree_entry in tree_entries {
        match tree_entry {
            Ok(entry) => println!("{}", entry.filename.to_str().unwrap()),
            Err(e) => println!("{}", e),
        }
    }

    Ok(())
}

fn mmap_dat(dat_path_str: &str) -> io::Result<Mmap> {
    let dat_path = Path::new(&dat_path_str);
    if dat_path.exists() {
        let dat_file = File::open(dat_path)?;
        unsafe { Mmap::map(&dat_file) }
    } else {
        let err_msg = format!("{}: no such file", dat_path_str);
        return Err(Error::new(ErrorKind::NotFound, err_msg));
    }
}

fn find_entries(dat_file: &[u8]) -> io::Result<TreeEntryIterator> {
    let len = dat_file.len();

    if len < DAT_FILE_MIN_SIZE {
        let err_msg = format!("is too small: must be at least 8 bytes long");
        return Err(Error::new(ErrorKind::InvalidData, err_msg));
    }

    let file_size =
        LittleEndian::read_u32(&dat_file[len-4..len]) as usize;

    if file_size != len {
        let err_msg = format!("size on disk ({}) doesn't match size from dat_file size field ({})", len, file_size);
        return Err(Error::new(ErrorKind::InvalidData, err_msg));
    }

    let tree_size =
        LittleEndian::read_u32(&dat_file[len-8..len-4]) as usize;

    if tree_size + DAT_FILE_FOOTER_BYTES > file_size {
        let err_msg = format!("size on disk ({}) is too small to fit the tree data ({}) plus footers", len, tree_size);
        return Err(Error::new(ErrorKind::InvalidData, err_msg));
    }

    let tree_entries_start = len - tree_size - 4;
    let tree_entries_end = len - DAT_FILE_FOOTER_BYTES;

    let tree_entries_data = &dat_file[tree_entries_start..tree_entries_end];

    Ok(read_tree_entries(tree_entries_data))
}

pub fn extract_all_entries(dat_path: &str, output_dir: &str) -> io::Result<()> {
    let output_dir = Path::new(&output_dir);

    if !output_dir.exists() {
        let err_msg = format!("{}: no such directory", output_dir.to_str().unwrap());
        return Err(Error::new(ErrorKind::NotFound, err_msg));
    } else if !output_dir.is_dir() {
        let err_msg = format!("{}: not a directory", output_dir.to_str().unwrap());
        return Err(Error::new(ErrorKind::InvalidInput, err_msg));
    } else {
        let dat_data = mmap_dat(dat_path)?;
        let tree_entries = find_entries(&dat_data)?;

        for tree_entry in tree_entries {
            let tree_entry = tree_entry?;
            let entry_data = get_entry_data(&dat_data, &tree_entry)?;
            let output_path = output_dir.join(&tree_entry.filename);
            write_data(&entry_data, &output_path)?;
        }
    }

    Ok(())
}

fn get_entry_data<'a>(dat_data: &'a [u8], entry: &TreeEntry) -> io::Result<&'a [u8]> {
    let data_start = entry.offset;
    let data_end = data_start + entry.packed_size;

    match dat_data.get(data_start..data_end) {
        Some(entry_data) => {
            Ok(entry_data)
        },
        None => {
            let err_msg = format!("{}: data range ({}-{}) is out of bounds", entry.filename.to_str().unwrap(), data_start, data_end);
            Err(Error::new(ErrorKind::InvalidData, err_msg))
        }
    }
}

fn write_data(entry_data: &[u8], output_path: &Path) -> io::Result<()> {
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

fn is_zlib_compressed(data: &[u8]) -> bool {
    data.len() > 2 &&
        data[0] == ZLIB_FIRST_MAGIC_BYTE &&
        data[1] == ZLIB_SECOND_MAGIC_BYTE
}
