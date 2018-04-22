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
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("remaining tree data is too small to actually fit a tree entry");
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
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
            let err_kind = std::io::ErrorKind::InvalidData;
            let err_msg = format!("cannot decode filename as ASCII");
            let err = std::io::Error::new(err_kind, err_msg);
            Err(err)
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
        let err_kind = std::io::ErrorKind::NotFound;
        let err_msg = format!("{}: no such file", dat_path_str);
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }
}

fn find_entries(dat_file: &[u8]) -> io::Result<TreeEntryIterator> {
    let len = dat_file.len();

    if len < DAT_FILE_MIN_SIZE {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("is too small: must be at least 8 bytes long");
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }

    let file_size =
        LittleEndian::read_u32(&dat_file[len-4..len]) as usize;

    if file_size != len {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("size on disk ({}) doesn't match size from dat_file size field ({})", len, file_size);
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }

    let tree_size =
        LittleEndian::read_u32(&dat_file[len-8..len-4]) as usize;

    if tree_size + DAT_FILE_FOOTER_BYTES > file_size {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("size on disk ({}) is too small to fit the tree data ({}) plus footers", len, tree_size);
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }

    let tree_entries_start = len - tree_size - 4;
    let tree_entries_end = len - DAT_FILE_FOOTER_BYTES;

    let tree_entries_data = &dat_file[tree_entries_start..tree_entries_end];

    Ok(read_tree_entries(tree_entries_data))
}

pub fn extract(dat_path: &str, output_path: &str) -> io::Result<()> {
    let output_path = Path::new(&output_path);

    if !output_path.exists() {
        let err_kind = std::io::ErrorKind::NotFound;
        let err_msg = format!("{}: no such directory", output_path.to_str().unwrap());
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    } else if !output_path.is_dir() {
        let err_kind = std::io::ErrorKind::InvalidInput;
        let err_msg = format!("{}: not a directory", output_path.to_str().unwrap());
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    } else {
        let dat_path = Path::new(&dat_path);
        let dat_file = mmap_dat(dat_path.to_str().unwrap())?;
        let tree_entries = find_entries(&dat_file)?;

        for tree_entry in tree_entries {
            let tree_entry = tree_entry?;
            extract_entry(&dat_file, &output_path, tree_entry)?;
        }

        Ok(())
    }
}

fn get_data_slice_for_entry<'a>(dat_data: &'a[u8], entry: &TreeEntry) -> io::Result<&'a[u8]> {
    if entry.offset > dat_data.len() {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("{}: start offset ({}) is outside of the data's bounds", entry.filename.to_str().unwrap(), entry.offset);
        let err = std::io::Error::new(err_kind, err_msg);
        Err(err)
    } else if entry.offset + entry.packed_size > dat_data.len() {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("{}: end index({}) is outside the data's bounds", entry.filename.to_str().unwrap(), entry.offset + entry.packed_size);
        let err = std::io::Error::new(err_kind, err_msg);
        Err(err)
    } else {
        Ok(&dat_data[entry.offset..entry.offset+entry.packed_size])
    }
}

fn extract_entry(dat_data: &[u8], output_dir: &Path, entry: TreeEntry) -> io::Result<()> {
    let entry_data = get_data_slice_for_entry(&dat_data, &entry)?;
    let output_path = output_dir.join(&entry.filename);

    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    if output_path.exists() {
        eprintln!("{}: already exists: skipping", output_path.to_str().unwrap());
    } else {
        write_entry(&entry_data, &output_path)?;
    }

    Ok(())
}

fn write_entry(data: &[u8], output_path: &Path) -> io::Result<()> {
    let mut output_file = std::fs::File::create(&output_path)?;

    // Ignore the `is_compressed` flag and just check for zlib magic number:
    // because the flag can be incorrect.
    if data.len() > 2 && data[0] == 0x78 && data[1] != 0xda {
        let mut zlib_reader = ZlibDecoder::new(data);
        std::io::copy(&mut zlib_reader, &mut output_file)?;
    } else {
        output_file.write(data)?;
    }

    Ok(())
}
