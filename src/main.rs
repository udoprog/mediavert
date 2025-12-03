//! ## bookvert
//!
//! This is a .cbz batch conversion tool which scans directories for image
//! files, groups them by their directory and creates books out of them.

use core::str::FromStr;
use core::{fmt, iter};
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
    /// When there are more than one book, specify a predicate for how to pick.
    ///
    /// Format: `[from=]to` where `from` is an optional book number to match,
    /// and `to` is one of `first`, `last`, or an exact index (0-based).
    #[arg(long, short = 'p')]
    pick: Vec<String>,
    /// Directories to convert.
    path: Vec<PathBuf>,
}

struct Book<'a> {
    path: &'a Path,
    name: &'a str,
    pages: Vec<(PathBuf, String)>,
    numbers: BTreeSet<u32>,
}

enum To {
    First,
    Last,
    Exact(usize),
}

impl To {
    /// Picks a book from the list according to the strategy.
    fn pick<'book, 'a>(&self, books: &[&'book Book<'a>]) -> Option<&'book Book<'a>> {
        match self {
            To::First => books.first().copied(),
            To::Last => books.last().copied(),
            To::Exact(n) => books.get(*n).copied(),
        }
    }
}

impl FromStr for To {
    type Err = anyhow::Error;

    #[inline]
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "first" => Ok(To::First),
            "last" => Ok(To::Last),
            _ => {
                let n: usize = s
                    .parse()
                    .with_context(|| anyhow!("Parsing pick target '{}'", s))?;
                Ok(To::Exact(n))
            }
        }
    }
}

impl fmt::Display for To {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            To::First => write!(f, "first"),
            To::Last => write!(f, "last"),
            To::Exact(n) => write!(f, "{}", n),
        }
    }
}

struct Predicate {
    /// The predicate only applies to the specified book number.
    from: Option<u32>,
    what: To,
}

struct Pick {
    predicates: Vec<Predicate>,
}

impl Pick {
    /// Parse a predicate to add to the picker.
    fn parse(&mut self, input: &str) -> Result<()> {
        for p in input.split(',') {
            let p = p.trim();

            if let Some((from, to)) = p.split_once('=') {
                let from = from.trim().parse()?;
                let to = to.trim().parse()?;

                self.predicates.push(Predicate {
                    from: Some(from),
                    what: to,
                });
            } else {
                let to = p.parse()?;

                self.predicates.push(Predicate {
                    from: None,
                    what: to,
                });
            }
        }

        Ok(())
    }

    /// Returns the index of the book to pick, or None if no predicate matched.
    fn pick<'book, 'a>(&self, number: u32, books: &[&'book Book<'a>]) -> Option<&'book Book<'a>> {
        if let [book] = books {
            return Some(*book);
        }

        for predicate in &self.predicates {
            let Some(from) = predicate.from else {
                return predicate.what.pick(books);
            };

            if number == from {
                if let Some(book) = predicate.what.pick(books) {
                    return Some(book);
                }
            }
        }

        None
    }
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

    let mut pick = Pick {
        predicates: Vec::new(),
    };

    for predicate in &opts.pick {
        pick.parse(&predicate)
            .with_context(|| anyhow!("Parsing pick predicate '{}'", predicate))?;
    }

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

    let mut books_by_path = BTreeMap::<&Path, Book<'_>>::new();

    for from in &files {
        let Some(path) = from.parent() else {
            continue;
        };

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        let book = books_by_path.entry(path).or_insert_with(|| Book {
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
        books_by_path.retain(|path, _| {
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

    for book in books_by_path.values() {
        for &n in book.numbers.iter() {
            by_number.entry(n).or_default().push(book);
        }
    }

    for (number, books) in &by_number {
        let number = *number;

        let Some(book) = pick.pick(number, &books) else {
            o.set_color(&error)?;
            write!(o, "[error] ")?;
            o.reset()?;

            writeln!(o, "{number:03}: more than one match, use -p")?;

            for (index, book) in books.iter().enumerate() {
                let pick = match index {
                    0 => To::First,
                    n if n + 1 == books.len() => To::Last,
                    n => To::Exact(n),
                };

                writeln!(o, "  `-p {number}={pick}`: {}", book.path.display())?;
            }

            continue;
        };

        let mut target = opts.out.clone();

        match &opts.rename {
            Some(name) => {
                target.push(format!("{}{number}", name));
            }
            None => {
                target.push(book.name);
            }
        }

        target.add_extension("cbz");

        let color = if opts.dry_run { &warn } else { &ok };
        o.set_color(color)?;
        write!(o, "[from]")?;
        o.reset()?;

        writeln!(o, " {number:03}: {}", book.path.display())?;

        if target.exists() && !opts.force {
            o.set_color(&warn)?;
            write!(o, "  [exists] ")?;
            o.reset()?;
            writeln!(o, "{} (--force to overwrite)", target.display())?;
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
            write!(o, "  [dry-run] ")?;
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
