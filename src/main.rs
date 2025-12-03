//! ## bookvert
//!
//! This is a .cbz batch conversion tool which scans directories for image
//! files, groups them by their directory and creates books out of them.

use core::str::FromStr;
use core::{fmt, iter};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
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
    /// Rename output files to this name. This is necessary if we are converting
    /// a series. Otherwise the directory name will be used.
    #[arg(long)]
    name: Option<String>,
    /// Overwrite existing files.
    #[arg(long)]
    force: bool,
    /// Perform a trial run with no changes made.
    #[arg(long)]
    dry_run: bool,
    /// Specify a regular expression for a name to skip.
    #[arg(long)]
    skip: Vec<String>,
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

enum From {
    Full,
    Single(u32),
    RangeInclusive(u32, u32),
    Range(u32, u32),
    RangeOpen(u32),
}

impl From {
    /// Returns true if the book number matches the predicate.
    fn matches(&self, number: u32) -> bool {
        match *self {
            From::Full => true,
            From::Single(n) => n == number,
            From::RangeInclusive(start, end) => (start..=end).contains(&number),
            From::Range(start, end) => (start..end).contains(&number),
            From::RangeOpen(start) => (start..).contains(&number),
        }
    }
}

impl FromStr for From {
    type Err = anyhow::Error;

    #[inline]
    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim();

        if s == ".." {
            return Ok(From::Full);
        }

        if let Some((from, to)) = s.split_once("..=") {
            let from = from.trim().parse()?;
            let to = to.trim().parse()?;
            return Ok(From::RangeInclusive(from, to));
        };

        if let Some((from, to)) = s.split_once("..") {
            let from = from.trim().parse()?;
            let to = to.trim();

            if to.is_empty() {
                return Ok(From::RangeOpen(from));
            }

            let to = to.parse()?;
            return Ok(From::Range(from, to));
        };

        Ok(From::Single(s.trim().parse()?))
    }
}

struct Match {
    /// The predicate only applies to the specified book number.
    from: From,
    /// The target to pick if the predicate matches.
    to: To,
}

#[derive(Default)]
struct Picker {
    matches: Vec<Match>,
    catch_all: Vec<To>,
}

impl Picker {
    /// Parse a predicate to add to the picker.
    fn parse(&mut self, input: &str) -> Result<()> {
        for p in input.split(',') {
            let p = p.trim();

            if let Some((from, to)) = p.rsplit_once('=') {
                let from = from.trim().parse()?;
                let to = to.trim().parse()?;

                self.matches.push(Match { from, to });
            } else {
                self.catch_all.push(p.parse()?);
            }
        }

        Ok(())
    }

    /// Returns the index of the book to pick, or None if no predicate matched.
    fn pick<'book, 'a>(&self, number: u32, books: &[&'book Book<'a>]) -> Option<&'book Book<'a>> {
        if let [book] = books {
            return Some(*book);
        }

        for m in &self.matches {
            if m.from.matches(number) {
                if let Some(book) = m.to.pick(books) {
                    return Some(book);
                }
            }
        }

        for what in &self.catch_all {
            if let Some(book) = what.pick(books) {
                return Some(book);
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

    let mut picker = Picker::default();

    for predicate in &opts.pick {
        picker
            .parse(&predicate)
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
        books_by_path.retain(|_, book| !skip.iter().any(|re| re.is_match(book.name)));
    }

    let o = termcolor::StandardStream::stdout(termcolor::ColorChoice::Auto);
    let mut o = o.lock();

    let mut by_number = BTreeMap::<_, Vec<_>>::new();
    let mut names = BTreeSet::new();

    for book in books_by_path.values() {
        for &n in book.numbers.iter() {
            by_number.entry(n).or_default().push(book);
        }

        names.insert(book.name);
    }

    let name = 'name: {
        if let Some(name) = &opts.name {
            break 'name name.clone();
        }

        let mut it = names.iter();

        if let Some(first) = it.next() {
            if it.next().is_none() {
                break 'name first.to_string();
            }
        }

        o.set_color(&error)?;
        write!(o, "[error] ")?;
        o.reset()?;

        writeln!(o, "Use `--name <name>` to set one name of the series:")?;

        for name in &names {
            writeln!(o, "  {}", escape(&name))?;
        }

        bail!("Aborting due to previous issues.");
    };

    let mut picked = Vec::new();
    let mut errors = 0;

    for (number, books) in &by_number {
        let number = *number;

        let Some(book) = picker.pick(number, &books) else {
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

            errors += 1;
            continue;
        };

        picked.push((number, book));
    }

    ensure!(errors == 0, "Aborting due to previous issues.");

    for (number, book) in picked {
        let mut target = opts.out.clone();
        target.push(format!("{name}{number}"));
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

fn escape(input: &str) -> Cow<'_, str> {
    let mut escaped = String::new();

    let n = 'escape: {
        // If we encounter a character that has to be escaped, copy it to the
        // escaped string and switch to escaped processing.
        for (n, c) in input.char_indices() {
            if !matches!(c, '"' | '\\') && !c.is_whitespace() {
                continue;
            }

            escaped.push('"');
            escaped.push_str(&input[..n]);
            break 'escape n;
        }

        return Cow::Borrowed(input);
    };

    for c in input[n..].chars() {
        let mut dst = [0u8; 4];

        escaped.push_str(match c {
            '"' => "\\\"",
            '\\' => "\\\\",
            c => c.encode_utf8(&mut dst),
        });
    }

    escaped.push('"');
    Cow::Owned(escaped)
}
