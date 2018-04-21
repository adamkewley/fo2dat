# fo2dat

A file archiver for Fallout2 DAT files


**DOES NOT WORK**:
- It's a rust learning exercise
- I generally learn languages by implementing data/protocol specs
- It's focused on structuring, building, and deploying a simple Rust application so I
  can see how the overall pipeline (not just the lang) would fit into my interests.
- I usually write documentation before the application; therefore, ignore the
  documentation for now


# Usage

Ripped from `tar`, because devs are already familiar and `7z` has a more complicated
CLI.

```bash
fo2dat -cf master.dat master/*  # create master.dat from files in master/
fo2dat -tvf master.dat          # list all files in master.dat
fo2dat -xf master.dat           # extract files from master.dat
```


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
- In C:

```C
struct tree_entry {
    uint32_t filename_len;
    char     filename[filename_len];
    uint8_t  is_compressed;
    uint32_t decompressed_size;
    uint32_t packed_size;
    uint32_t offset;
};
```


Sources:


- http://falloutmods.wikia.com/wiki/DAT_file_format
- http://fallout.wikia.com/wiki/DAT_files
- Hacker that reverse-engineered the spec:  MatuX (matip@fibertel.com.ar)
- I rewrote the spec to more closely resemble an RFC data spec