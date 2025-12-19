//! [<img alt="github" src="https://img.shields.io/badge/github-udoprog/mediavert-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/mediavert)
//! [<img alt="crates.io" src="https://img.shields.io/crates/v/audiovert.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/audiovert)
//! [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-audiovert-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/audiovert)
//!
//! A tool to perform batch conversion of music.
//!
//! Any arguments to the conversion tool will be treated as a directory that
//! will be recursively scanned for files to convert.
//!
//! By default, lossless formats will be converted to mp3 at `320kbps`, and any
//! lossy files will be hard linked to the target directory, but the exact
//! behavior can be configured using commandline arguments.
//!
//! Unless `--to <dir>` is specified, conversions are performed in-placed, the
//! source file will not be moved unless `--trash-source` or `--remove-source`
//! is specified.
//!
//! If any archives are encountered (zip, rar, 7z), they will be extracted
//! in-memory and treated as-if they are files inside of a folder named the same
//! as the archive.
//!
//! So if you have an archive like `music.zip` containing `song1.flac` and
//! `song2.flac` it will be treated as if it was a directory like:
//!
//! ```text
//! input/
//!   # music.zip (archive)
//!   music/
//!     song1.flac
//!     song2.flac
//! ```
//!
//! <br>
//!
//! ## Usage
//!
//! It is generally recommended to first run the command with `--dry-run` or
//! `-D` to get an understanding of what it will try to do:
//!
//! ```sh
//! toolkit --dry-run unsorted --to sorted
//! ```
//!
//! Once this looks good, you can run the command without `--dry-run`.
//!
//! ```sh
//! toolkit --to sorted
//! ```

#![allow(clippy::drain_collect)]

mod archive;
mod bitrates;
pub mod cli;
mod condition;
mod config;
mod format;
mod link;
mod meta;
mod out;
mod set_bit_rate;
mod shell;
mod tasks;
