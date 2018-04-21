# fo2dat

A file archiver for Fallout 2 "DAT2" files


# Usage

```
# show help
$ fo2dat --help

# list contents of master.dat
$ fo2dat -tf master.dat

# extract master.dat into current dir
$ fo2dat -xf master.dat

# extract master.dat into fo2/
$ mkdir fo2
$ fo2dat -xf master.dat -C fo2
```

**Note**:
- This utility will decompresses zlib-compressed files when it can; however, typical Fallout 2
  DAT2 files contain erroneous compression flags etc. When `fo2dat` cannot be certain that a
  file is compressed, it skips decompression
- Most Fallout2 DAT2 files contain duplicate entries, `fo2dat` skips duplicates as they are
  encountered (i.e. first instance of an entry is extracted, 2nd does not overwrite)


# DAT Spec


## `dat_file`

```text

    0                   1                   2                   3
    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                               .                               |
   |                               .                               |
   |                             data                              |
   |     (len = sum(entry.packed_size for entry tree_entries))     |
   |                               .                               |
   |                               .               ----------------|
   |                               .               |      0x0      |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                               .                               |
   |                               .                               |
   |                          tree_entries                         |
   |                   (size in bytes = tree_size)                 |
   |                               .                               |
   |                               .                               |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                           tree_size                           |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                           file_size                           |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

```

- Top-level container for all archive data
- All multi-byte numbers are little-endian
- `data` and `tree_entries` are variable length. `tree_size` holds the size (in bytes)
  of `tree_entries`. The size of `data` can be calculated by adding the `packed_size`
  of each `tree_entry` in `tree_entries`


## `data`

- Contains the data for all files described in `tree_entries`
- The data of all files is concatenated together with no separators
- The offset and size of each file in `data` is described in the file's respective
  `tree_entry` in `tree_entries`
- A file's data MAY use zlib compression, which is indicated by its first two bytes
  having the zlib magic number: `0x78da`
- Some DAT files appear to contain duplicate entries


## `tree_entries`

- Contains metadata for each file in `dat_file`
- A continuous block of `tree_entry`s with no separators
- The starting offset for `tree_entries` can be calculated from `tree_size`


## `tree_entry`

```text

    0                   1                   2                   3
    0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                         filename_len                          |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                               .                               |
   |                               .                               |
   |                            filename                           |
   |                   (ASCII, len = filename_len)                 |
   |                               .               ----------------|
   |                               .               | is_compressed |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                        decompressed_size                      |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                          packed_size                          |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                            offset                             |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

```

- Contains metadata of a file in `dat_file`
- The data of the file is held in `dat_file`s `data` block, starting at
  `offset` and ending at `offset + packed_size`
- `is_compressed` can have a value of either `0x0` (uncompressed) or `0x1`
  (compressed)


Sources:


- http://falloutmods.wikia.com/wiki/DAT_file_format
- http://fallout.wikia.com/wiki/DAT_files
- Hacker that reverse-engineered the spec:  MatuX (matip@fibertel.com.ar)
- I rewrote the spec to more closely resemble an RFC data spec
