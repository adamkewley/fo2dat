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

/// The first state of the DAT parser, reached by calling `DatSectionsFoundState::from_bytes` on a
/// byte slice.
///
/// DAT files are split into:
///
/// - A data section (variable size - calculated by knowing the offsets of the tree section)
/// - A tree section (variable size - calculated by reading the DAT file's footers)
/// - Metadata (num files, tree size, file size - known as part of the DAT spec)
///
/// Knowing the offsets + size of each section is a requirement for parsing the more-useful
/// higher-level entries (e.g. files in the DAT). This state represents when the parser has
/// calculated those values.
#[allow(dead_code)]
pub struct DatSectionsFoundState<'a> {
    file_data: &'a [u8],
    num_files: usize,
    tree_entry_data: &'a [u8],
    file_size: usize,
}

impl <'a> DatSectionsFoundState<'a> {

    /// Attempt to enter the first parsing state by parsing the supplied raw byte slice as DAT2 data.
    ///
    /// This operation is cheap, because it's only parsing section offsets, which only requires
    /// reading small, scattered fields from the supplied byte slice.
    pub fn from_bytes(bytes: &[u8]) -> io::Result<DatSectionsFoundState> {
        const NUM_FILES_BYTES: usize = 4;
        const TREE_SIZE_BYTES: usize = 4;
        const FILE_SIZE_BYTES: usize = 4;
        const NUM_FOOTER_BYTES: usize = TREE_SIZE_BYTES + FILE_SIZE_BYTES;
        const MIN_SIZE: usize = NUM_FILES_BYTES + NUM_FOOTER_BYTES;

        let len = bytes.len();

        if len < MIN_SIZE {
            let err_msg = format!("is too small: must be at least 8 bytes long");
            return Err(Error::new(ErrorKind::InvalidData, err_msg));
        }

        let file_size =
            LittleEndian::read_u32(&bytes[len-FILE_SIZE_BYTES..]) as usize;

        if file_size != len {
            let err_msg = format!("size of data ({}) doesn't match size from the dat_file size field ({})", len, file_size);
            return Err(Error::new(ErrorKind::InvalidData, err_msg));
        }

        let tree_end = len - NUM_FOOTER_BYTES;

        let tree_size =
            LittleEndian::read_u32(&bytes[tree_end..][..TREE_SIZE_BYTES]) as usize - TREE_SIZE_BYTES;

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
            LittleEndian::read_u32(&bytes[num_files_start..][..NUM_FILES_BYTES]) as usize;

        Ok(DatSectionsFoundState {
            file_data: &bytes[0..num_files_start],
            num_files,
            tree_entry_data: &bytes[tree_start..tree_end],
            file_size,
        })
    }

    /// Returns an iterator that emits each `TreeEntry` parsed from the original input byte slice.
    pub fn iter_tree_entries(&'a self) -> Box<Iterator<Item = io::Result<TreeEntry>> + 'a> {
        let offset: usize = 0;

        let iter = (0..std::usize::MAX).scan(offset, move |offset, _| {
            if *offset >= self.tree_entry_data.len() {
                return None
            }

            match  TreeEntry::from_bytes(&self.tree_entry_data[*offset..]) {
                Ok((entry, entry_size)) => {
                    *offset += entry_size;
                    Some(Ok(entry))
                },
                Err(e) => {
                    *offset = std::usize::MAX; // makes next iter ret None
                    Some(Err(e))
                }
            }
        });

        Box::new(iter)
    }

    /// Returns an iterator that emits each `EntryData` parsed from the original input byte slice.
    ///
    /// Note: This emits the entry's data as-is - it does *not* automatically decompress the data.
    pub fn iter_data(&'a self) -> Box<Iterator<Item = io::Result<DataEntry<'a>>> + 'a> {
        Box::new(self.iter_tree_entries().map(move |entry| {
            get_data(&self.file_data, entry?)
        }))
    }
}

/// An entry, as read from the `tree_entires` section of the input DAT file.
///
/// `TreeEntry`s are effectively metadata about an entry in the DAT file. The *actual* data is
/// located elsewhere in the DAT file (see: `iter_tree_entries`).
#[allow(dead_code)]
pub struct TreeEntry {
    pub filename: PathBuf,
    pub is_compressed: bool,
    pub decompressed_size: usize,
    pub packed_size: usize,
    pub offset: usize,
}

impl TreeEntry {

    /// Attempts to parse `data` as a tree entry. Returns the data as a `TreeEntry`, along with the
    /// number of bytes read to parse the returned `TreeEntry`.
    pub fn from_bytes(data: &[u8]) -> io::Result<(TreeEntry, usize)> {
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
}

/// A data entry (effectively, a file), as parsed from the DAT.
///
/// Note: The data is **not** decompressed. Users of this struct should check for zlib compression
/// and perform decompression, if necessary.
#[allow(dead_code)]
pub struct DataEntry<'a> {
    pub path: PathBuf,
    pub raw_data: &'a [u8],
    pub decompressed_size: usize,
}

fn get_data<'a>(dat_data: &'a [u8], entry: TreeEntry) -> io::Result<DataEntry<'a>> {
    let data_start = entry.offset;
    let data_end = data_start + entry.packed_size;

    match dat_data.get(data_start..data_end) {
        Some(entry_data) => {
            Ok(DataEntry {
                path: entry.filename,
                raw_data: entry_data,
                decompressed_size: entry.decompressed_size,
            })
        },
        None => {
            let err_msg = format!("{}: data range ({}-{}) is out of bounds", entry.filename.to_str().unwrap(), data_start, data_end);
            Err(Error::new(ErrorKind::InvalidData, err_msg))
        }
    }
}

/// Lists the contents of a DAT file to the standard output.
pub fn list_contents(dat_path_str: &str) -> io::Result<()> {
    let raw_dat_data = mmap_dat_file(dat_path_str)?;
    let dat_sections = DatSectionsFoundState::from_bytes(&raw_dat_data)?;
    let tree_entries = dat_sections.iter_tree_entries();

    for tree_entry in tree_entries {
        let tree_entry = tree_entry?;
        println!("{}", tree_entry.filename.to_str().unwrap());
    }

    Ok(())
}

fn mmap_dat_file(dat_path_str: &str) -> io::Result<Mmap> {
    let dat_path = Path::new(&dat_path_str);
    if dat_path.exists() {
        let dat_file = File::open(dat_path)?;
        unsafe { Mmap::map(&dat_file) }
    } else {
        let err_msg = format!("{}: no such file", dat_path_str);
        return Err(Error::new(ErrorKind::NotFound, err_msg));
    }
}

/// Extract all entries in a DAT file located at `dat_path` to `output_dir`
pub fn extract_all_entries(dat_path: &str, output_dir: &str) -> io::Result<()> {
    let output_dir = Path::new(&output_dir);

    if !output_dir.exists() {
        let err_msg = format!("{}: no such directory", output_dir.to_str().unwrap());
        return Err(Error::new(ErrorKind::NotFound, err_msg));
    } else if !output_dir.is_dir() {
        let err_msg = format!("{}: not a directory", output_dir.to_str().unwrap());
        return Err(Error::new(ErrorKind::InvalidInput, err_msg));
    } else {
        let raw_dat_data = mmap_dat_file(dat_path)?;
        let sections_found_state = DatSectionsFoundState::from_bytes(&raw_dat_data)?;
        let entries_data = sections_found_state.iter_data();

        for entry_data in entries_data {
            let entry_data = entry_data?;
            let output_path = output_dir.join(&entry_data.path);
            write_entry(entry_data.raw_data, &output_path)?;
        }
    }

    Ok(())
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
pub fn is_zlib_compressed(data: &[u8]) -> bool {
    const ZLIB_FIRST_MAGIC_BYTE: u8 = 0x78;
    const ZLIB_SECOND_MAGIC_BYTE: u8 = 0xda;

    data.len() > 2 && data[0] == ZLIB_FIRST_MAGIC_BYTE && data[1] == ZLIB_SECOND_MAGIC_BYTE
}
