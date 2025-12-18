//! A tool to perform batch conversion of audio.
//! 
//! See [`bookvert`] documentation for more information.
//!
//! [`bookvert`]: https://crates.io/crates/bookvert

use anyhow::Result;
use clap::Parser;

/// A tool to perform batch conversion of books.
#[derive(Parser)]
#[command(about, version, max_term_width = 80)]
struct Opts {
    #[command(flatten)]
    inner: bookvert::cli::Bookvert,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    bookvert::cli::entry(&opts.inner)
}
