//! ## bookvert
//!
//! This is a .cbz batch conversion tool which scans directories for image
//! files, groups them by their directory and creates books out of them.

use core::iter;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use ignore::Walk;
use regex::Regex;
use termcolor::{ColorSpec, WriteColor};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Helper tool to batch convert files into a .cbr
#[derive(Parser)]
#[command(about, version)]
struct Opts {
    /// Output directory to write to.
    #[arg(long, default_value = ".")]
    out: PathBuf,
    /// Overwrite existing files.
    #[arg(long)]
    force: bool,
    /// Perform a trial run with no changes made.
    #[arg(long)]
    dry_run: bool,
    /// Regular expressions for names to skip.
    #[arg(long)]
    skip: Vec<String>,
    /// Rename output files to this name, if a name cannot be determined.
    #[arg(long)]
    rename: Option<String>,
    /// Start numbering from this book when renaming.
    #[arg(long, default_value_t = 1)]
    start_book: usize,
    /// Use the first book found in case of duplicates.
    #[arg(long)]
    first_book: bool,
    /// Use the last book found in case of duplicates.
    #[arg(long)]
    last_book: bool,
    /// Use the first number found in the directory name.
    #[arg(long)]
    first_number: bool,
    /// Directories to convert.
    path: Vec<PathBuf>,
}

struct Book<'a> {
    path: &'a Path,
    name: &'a str,
    pages: Vec<(PathBuf, String)>,
    numbers: BTreeSet<u32>,
}

fn main() -> Result<()> {
    let mut warn: ColorSpec = ColorSpec::new();
    warn.set_fg(Some(termcolor::Color::Yellow));

    let mut ok: ColorSpec = ColorSpec::new();
    ok.set_fg(Some(termcolor::Color::Green));

    let mut error: ColorSpec = ColorSpec::new();
    error.set_fg(Some(termcolor::Color::Red));

    let opts = Opts::try_parse()?;

    let mut skip = Vec::<Regex>::new();

    for pat in &opts.skip {
        let re = Regex::new(pat).with_context(|| anyhow!("Parsing regex '{}'", pat))?;
        skip.push(re);
    }

    let mut files = Vec::new();

    for path in opts.path {
        for p in Walk::new(path) {
            let entry = p?;

            let Some(ty) = entry.file_type() else {
                continue;
            };

            if ty.is_file() {
                files.push(entry.into_path());
            }
        }
    }

    files.sort();

    let mut books = BTreeMap::<&Path, Book<'_>>::new();

    for from in &files {
        let Some(path) = from.parent() else {
            continue;
        };

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        let book = books.entry(path).or_insert_with(|| Book {
            path,
            name,
            pages: Vec::new(),
            numbers: numbers(name).collect(),
        });

        let Some(ext) = from.extension() else {
            continue;
        };

        let ext = ext.to_string_lossy().to_lowercase();

        let page = format!("p{:03}.{ext}", book.pages.len());
        book.pages.push((from.clone(), page));
    }

    if !skip.is_empty() {
        books.retain(|path, _| {
            for c in path.components() {
                let name = c.as_os_str().to_string_lossy();

                if skip.iter().any(|re| re.is_match(&name)) {
                    return false;
                }
            }

            true
        });
    }

    let o = termcolor::StandardStream::stdout(termcolor::ColorChoice::Auto);
    let mut o = o.lock();

    let mut by_number = BTreeMap::<_, Vec<_>>::new();
    let mut is_error = false;

    for book in books.values() {
        for n in book.numbers.iter() {
            by_number.entry(n).or_default().push(book);
        }
    }

    for (number, books) in &by_number {
        if books.len() <= 1 || opts.first_book || opts.last_book {
            continue;
        }

        o.set_color(&error)?;
        write!(o, "[error] ")?;
        o.reset()?;

        writeln!(o, "{number:03}: more than one book")?;

        is_error = true;

        for book in books {
            writeln!(o, "  {:?}: {}", book.numbers, book.path.display())?;
        }
    }

    if is_error {
        return Err(anyhow!("Could not unambiguously determine books"));
    }

    for (n, books) in by_number {
        let book = match &books[..] {
            &[.., last] if opts.last_book => last,
            &[first, ..] => first,
            _ => continue,
        };

        let mut target = opts.out.clone();

        match &opts.rename {
            Some(name) => {
                target.push(format!("{}{n}", name));
            }
            None => {
                target.push(book.name);
            }
        }

        target.add_extension("cbz");

        let color = if opts.dry_run { &warn } else { &ok };
        o.set_color(color)?;
        write!(o, "[from] ")?;
        o.reset()?;

        writeln!(o, "{}", book.path.display())?;

        if target.exists() && !opts.force {
            o.set_color(&warn)?;
            write!(o, "[exists] ")?;
            o.reset()?;
            writeln!(o, "{} (use --force to overwrite)", target.display())?;
            continue;
        }

        let mut w = ZipWriter::new(Cursor::new(Vec::new()));

        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o755);

        for (from, name) in book.pages.iter() {
            let content = fs::read(&from)
                .with_context(|| anyhow!("Failed to read file {}", from.display()))?;

            w.start_file(name, options)?;
            w.write_all(&content)?;
        }

        let out = w.finish()?.into_inner();

        if opts.dry_run {
            o.set_color(&warn)?;
            write!(o, "  [skip] ")?;
            o.reset()?;
        } else {
            o.set_color(&ok)?;
            write!(o, "  [file] ")?;
            o.reset()?;
        }

        writeln!(o, "{} ({} bytes)", target.display(), out.len())?;

        if opts.dry_run {
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| {
                anyhow!("Failed to create parent directory {}", parent.display())
            })?;
        }

        fs::write(&target, out)
            .with_context(|| anyhow!("Failed to write file {}", target.display()))?;
    }

    Ok(())
}

/// Extracts all numbers from the input string as an iterator.
fn numbers(mut input: &str) -> impl Iterator<Item = u32> {
    iter::from_fn(move || {
        loop {
            let n = input.find(char::is_numeric)?;
            input = input.get(n..)?;
            let end = input.find(|c: char| !c.is_numeric()).unwrap_or(input.len());
            let head;
            (head, input) = input.split_at_checked(end)?;

            if let Ok(number) = head.parse() {
                return Some(number);
            }
        }
    })
}
