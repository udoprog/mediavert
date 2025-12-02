use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Helper tool to batch convert files into a .cbr
#[derive(Parser)]
#[command(about, version)]
struct Opts {
    /// Directories to convert.
    #[arg(long)]
    path: Vec<PathBuf>,
    /// Output directory to write to.
    #[arg(long, default_value = ".")]
    out: PathBuf,
    /// Overwrite existing files.
    #[arg(long)]
    force: bool,
    /// Rename output files to this name.
    #[arg(long)]
    rename: Option<String>,
    /// Start numbering from this book when renaming.
    #[arg(long, default_value_t = 1)]
    start_book: usize,
}

#[derive(Default)]
struct Book {
    pages: Vec<(PathBuf, String)>,
}

fn main() -> Result<()> {
    let opts = Opts::try_parse()?;

    let mut files = Vec::new();

    for path in opts.path {
        for p in ignore::Walk::new(path) {
            let entry = p?;

            let Some(ty) = entry.file_type() else {
                continue;
            };

            if ty.is_file() {
                files.push(entry.into_path().canonicalize()?);
            }
        }
    }

    files.sort();

    let mut books = BTreeMap::<_, Book>::new();

    for from in &files {
        let Some(parent) = from.parent() else {
            continue;
        };

        let Some(name) = parent.file_name() else {
            continue;
        };

        let book = books.entry(name).or_default();

        let Some(ext) = from.extension() else {
            continue;
        };

        let ext = ext.to_string_lossy().to_lowercase();

        let page = format!("p{:03}.{ext}", book.pages.len());
        book.pages.push((from.clone(), page));
    }

    let count = opts.start_book;

    for (n, (name, book)) in books.into_iter().enumerate() {
        let mut target = PathBuf::from(&opts.out);

        match &opts.rename {
            Some(name) => {
                target.push(format!("{}{}", name, count + n));
            }
            None => {
                target.push(name);
            }
        }

        target.add_extension("cbz");

        if target.exists() && !opts.force {
            println!("Skipping existing file {}", target.display());
            continue;
        }

        let mut w = ZipWriter::new(Cursor::new(Vec::new()));

        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o755);

        for (from, name) in book.pages {
            let content = fs::read(&from)
                .with_context(|| anyhow!("Failed to read file {}", from.display()))?;

            w.start_file(name, options)?;
            w.write_all(&content)?;
        }

        let out = w.finish()?.into_inner();

        println!("Writing file {}", target.display());

        fs::write(&target, out)
            .with_context(|| anyhow!("Failed to write file {}", target.display()))?;
    }

    Ok(())
}
