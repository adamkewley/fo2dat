extern crate byteorder;
extern crate flate2;

use std::io;
use byteorder::{LittleEndian, ByteOrder};
use std::str;
use std::path::PathBuf;
use std::io::Error;
use std::io::ErrorKind;
use std::ops::Range;

#[allow(dead_code)]
struct DatTopLevelStructure {
    data: Range<usize>,
    num_files: usize,
    tree: Range<usize>,
    file_size: usize,
}

impl DatTopLevelStructure {

    fn parse(dat_data: &[u8]) -> io::Result<Self> {
        const NUM_FILES_BYTES: usize = 4;
        const TREE_SIZE_BYTES: usize = 4;
        const FILE_SIZE_BYTES: usize = 4;
        const NUM_FOOTER_BYTES: usize = TREE_SIZE_BYTES + FILE_SIZE_BYTES;
        const MIN_SIZE: usize = NUM_FILES_BYTES + NUM_FOOTER_BYTES;

        let len = dat_data.len();

        if len < MIN_SIZE {
            let err_msg = format!("is too small: must be at least 8 bytes long");
            return Err(Error::new(ErrorKind::InvalidData, err_msg));
        }

        let file_size =
            LittleEndian::read_u32(&dat_data[len-FILE_SIZE_BYTES..]) as usize;

        if file_size != len {
            let err_msg = format!("size of data ({}) doesn't match size from the dat_file size field ({})", len, file_size);
            return Err(Error::new(ErrorKind::InvalidData, err_msg));
        }

        let tree_end = len - NUM_FOOTER_BYTES;

        let tree_size =
            LittleEndian::read_u32(&dat_data[tree_end..][..TREE_SIZE_BYTES]) as usize - TREE_SIZE_BYTES;

        if tree_size > tree_end {
            let err_msg = format!("size of data ({}) is too small to fit tree entries", len);
            return Err(Error::new(ErrorKind::InvalidData, err_msg));
        }

        let tree_start = tree_end - tree_size;

        if tree_start < NUM_FILES_BYTES {
            let err_msg = format!("size of data ({}) is too small to fit the file count", len);
            return Err(Error::new(ErrorKind::InvalidData, err_msg));
        }

        let num_files_start = tree_start - NUM_FILES_BYTES;

        let num_files =
            LittleEndian::read_u32(&dat_data[num_files_start..][..NUM_FILES_BYTES]) as usize;

        Ok(DatTopLevelStructure {
            data: (0..num_files_start),
            num_files,
            tree: (tree_start..tree_end),
            file_size,
        })
    }
}

/// Returns an iterator that emits tree entries found in the supplied DAT2 data.
///
/// The iterator will emit an `Err` if the data is invalid, followed by halting.
pub fn iter_tree(dat_data: &[u8]) -> io::Result<TreeEntries> {
    let top_level_structure = DatTopLevelStructure::parse(&dat_data)?;
    Ok(TreeEntries {
        tree_data: &dat_data[top_level_structure.tree],
        offset: 0,
    })
}

/// An iterator that emits `TreeEntry`s parsed from the tree section of DAT data.
pub struct TreeEntries<'a> {
    tree_data: &'a [u8],
    offset: usize,
}

impl <'a> Iterator for TreeEntries<'a> {
    type Item = io::Result<TreeEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.tree_data.len() {
            return None
        }

        let tree_data = &self.tree_data[self.offset..];

        match TreeEntry::parse(tree_data) {
            Ok((entry, entry_size)) => {
                self.offset += entry_size;
                Some(Ok(entry))
            },
            Err(e) => {
                // halts iteration
                self.offset += std::usize::MAX;
                Some(Err(e))
            }
        }
    }
}

/// A tree entry, as parsed from the `tree_entires` section of the input DAT file.
pub struct TreeEntry {
    pub path: PathBuf,
    pub is_compressed: bool,
    pub decompressed_size: usize,
    pub packed_size: usize,
    pub offset: usize,
}

impl TreeEntry {

    /// Attempts to parse `data` as a tree entry. Returns the data as a `TreeEntry`, along with the
    /// number of bytes read to parse the returned `TreeEntry`.
    fn parse(data: &[u8]) -> io::Result<(Self, usize)> {
        const TREE_ENTRY_HEADER_SIZE: usize = 4;
        const TREE_ENTRY_FOOTER_SIZE: usize = 13;
        const TREE_ENTRY_MIN_SIZE: usize = TREE_ENTRY_HEADER_SIZE + TREE_ENTRY_FOOTER_SIZE;
        const TREE_ENTRY_PATH_SEPARATOR: char = '\\';

        if data.len() < TREE_ENTRY_MIN_SIZE {
            let err_msg = format!("remaining tree data is too small to actually fit a tree entry");
            return Err(Error::new(ErrorKind::InvalidData, err_msg));
        }

        let filename_len =
            LittleEndian::read_u32(&data[0..TREE_ENTRY_HEADER_SIZE]) as usize;

        let total_len = TREE_ENTRY_HEADER_SIZE + filename_len + TREE_ENTRY_FOOTER_SIZE;

        if data.len() < total_len {
            let err_kind = ErrorKind::InvalidData;
            let err_msg = format!("not enough space remaining in tree data to accommodate a filename + relevant footers");
            let err = Error::new(err_kind, err_msg);
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
            path: filename,
            is_compressed: data[0] > 0,
            decompressed_size: LittleEndian::read_u32(&footer_data[1..5]) as usize,
            packed_size: LittleEndian::read_u32(&footer_data[5..9]) as usize,
            offset: LittleEndian::read_u32(&footer_data[9..13]) as usize,
        };

        Ok((tree_entry, total_len))
    }
}

/// Returns an iterator that emits raw data entries found in the supplied DAT2 data.
pub fn iter_data(dat_data: &[u8]) -> io::Result<DataEntries> {
    let top_level_structure = DatTopLevelStructure::parse(dat_data)?;
    Ok(DataEntries {
        data_section: &dat_data[top_level_structure.data],
        tree_entries: iter_tree(dat_data)?,
    })
}

pub struct DataEntries<'a> {
    data_section: &'a [u8],
    tree_entries: TreeEntries<'a>,
}

impl <'a> Iterator for DataEntries<'a> {
    type Item = io::Result<DataEntry<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.tree_entries.next()? {
            Ok(tree_entry) => {
                Some(get_data(self.data_section, tree_entry))
            },
            Err(e) => {
                Some(Err(e))
            },
        }
    }
}

/// A data entry (effectively, a file), as parsed from the DAT.
pub struct DataEntry<'a> {
    pub path: PathBuf,
    pub raw_data: &'a [u8],
    pub decompressed_size: usize,
}

fn get_data<'a>(data_section_data: &'a [u8], entry: TreeEntry) -> io::Result<DataEntry<'a>> {
    let data_start = entry.offset;
    let data_end = data_start + entry.packed_size;

    match data_section_data.get(data_start..data_end) {
        Some(entry_data) => {
            Ok(DataEntry {
                path: entry.path,
                raw_data: entry_data,
                decompressed_size: entry.decompressed_size,
            })
        },
        None => {
            let err_msg = format!("{}: data range ({}-{}) is out of bounds", entry.path.to_str().unwrap(), data_start, data_end);
            Err(Error::new(ErrorKind::InvalidData, err_msg))
        }
    }
}
