//! A tool to perform batch conversion of audio.
//! 
//! See [`audiovert`] documentation for more information.
//!
//! [`audiovert`]: https://crates.io/crates/audiovert

use anyhow::Result;
use clap::Parser;

/// A tool to perform batch conversion of audio.
#[derive(Parser)]
#[command(author, version, about, max_term_width = 80)]
pub struct Opts {
    #[command(flatten)]
    inner: audiovert::cli::Audiovert,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    audiovert::cli::entry(&opts.inner)
}
