extern crate memmap;
extern crate byteorder;

use std::io;
use std::path::Path;
use std::fs::File;
use memmap::Mmap;
use byteorder::{LittleEndian, ByteOrder};
use std::str;

const NUM_FOOTER_BYTES: usize = 8;
const DAT_FILE_MIN_SIZE: usize = NUM_FOOTER_BYTES;
const TREE_ENTRY_HEADER_SIZE: usize = 4;
const TREE_ENTRY_FOOTER_SIZE: usize = 13;
const TREE_ENTRY_MIN_SIZE: usize =
    TREE_ENTRY_HEADER_SIZE + TREE_ENTRY_FOOTER_SIZE;

pub struct TreeEntry {
    filename: String,
    is_compressed: bool,
    decompressed_size: u32,
    packed_size: u32,
    offset: u32,
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

    let filename_bytes = (&data[TREE_ENTRY_HEADER_SIZE + 1..filename_len]).to_owned();

    let filename = match String::from_utf8(filename_bytes) {
        Ok(s) => Ok(s),
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
        decompressed_size: LittleEndian::read_u32(&footer_data[1..5]),
        packed_size: LittleEndian::read_u32(&footer_data[5..9]),
        offset: LittleEndian::read_u32(&footer_data[9..13]),

    };

    Ok((tree_entry, total_len))
}

pub fn run(dat_path_str: &String) -> io::Result<()> {
    let dat_path = Path::new(dat_path_str);
    let dat_file = File::open(dat_path)?;
    let dat_file = unsafe { Mmap::map(&dat_file)? };

    let len = dat_file.len();

    if len < DAT_FILE_MIN_SIZE {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("{}: is too small: must be at least 8 bytes long", dat_path_str);
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }

    let file_size =
        LittleEndian::read_u32(&dat_file[len-4..len]) as usize;

    if file_size != len {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("{}: size on disk ({}) doesn't match size from dat_file size field ({})", dat_path_str, len, file_size);
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }

    let tree_size =
        LittleEndian::read_u32(&dat_file[len-8..len-4]) as usize;

    if tree_size + NUM_FOOTER_BYTES > file_size {
        let err_kind = std::io::ErrorKind::InvalidData;
        let err_msg = format!("{}: size on disk ({}) is too small to fit the tree data ({}) plus footers", dat_path_str, len, tree_size);
        let err = std::io::Error::new(err_kind, err_msg);
        return Err(err);
    }

    let tree_entries_data =
        &dat_file[len-tree_size-4..len-NUM_FOOTER_BYTES];

    let tree_entries = read_tree_entries(tree_entries_data);

    for tree_entry in tree_entries {
        match tree_entry {
            Ok(entry) => println!("{}", entry.filename),
            Err(e) => println!("{}", e),
        }
    }

    Ok(())
}
