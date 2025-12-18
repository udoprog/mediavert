use core::fmt::{self, Write as _};
use core::iter;
use core::str::FromStr;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use ignore::Walk;
use language_tags::LanguageTag;
use regex::Regex;
use termcolor::{ColorSpec, StandardStream, WriteColor};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::{App, Book, Catalog, Page, State};

/// A tool to perform batch conversion of books.
#[derive(Parser)]
pub struct Bookvert {
    /// Output directory to write to.
    #[arg(long, default_value = ".")]
    out: PathBuf,
    /// Rename output files to this name. This is necessary if we are converting
    /// a series. Otherwise the directory name will be used.
    #[arg(long)]
    name: Option<String>,
    /// When there are more than one book, specify a predicate for how to pick.
    ///
    /// Format: `[from=]to` where `from` is an book number or range to match.
    ///
    /// The range in `from` is specified as `n..m` (exclusive), `n..=m` (inclusive), or `n..` (open-ended) or `..` (all).
    /// The `to` target can be `first`, `last`, `most-pages`, a zero-based index, or a regular expression for the exact match to pick.
    ///
    /// Examples:
    /// - `-p most-pages` picks the match with the most pages for all books.
    /// - `-p 3=first` picks the first match for book number 3.
    /// - `-p 3=1` picks the second match for book number 3.
    /// - `-p 1..=5=most-pages` picks the match with the most pages for books 1 through 5.
    /// - `-p fix' will match *any* book that contains the string `fix`.
    #[arg(long, short = 'p', verbatim_doc_comment)]
    pick: Vec<String>,
    /// Overwrite existing files.
    #[arg(long, short = 'f')]
    force: bool,
    /// Non-interactive mode: errors out if a choice is required.
    #[arg(long, short = 'n')]
    noninteractive: bool,
    /// Verbose output.
    #[arg(long, short = 'v')]
    verbose: bool,
    /// Perform a trial run with no changes made.
    #[arg(long)]
    dry_run: bool,
    /// Specify a regular expression for a name to skip.
    #[arg(long)]
    skip: Vec<String>,
    /// Only include series numbers matching these predicates.
    #[arg(long)]
    include: Vec<From>,
    /// Series for ComicInfo.xml metadata.
    #[arg(long)]
    series: Option<String>,
    /// Writer / Author for ComicInfo.xml metadata.
    #[arg(long, alias = "writer")]
    author: Option<String>,
    /// Penciller for ComicInfo.xml metadata.
    #[arg(long, alias = "penciller")]
    artist: Option<String>,
    /// Publisher for ComicInfo.xml metadata.
    #[arg(long)]
    publisher: Option<String>,
    /// Genre for ComicInfo.xml metadata (comma-separated).
    #[arg(long)]
    genre: Option<String>,
    /// Language ISO code for ComicInfo.xml metadata (e.g., "en", "ja").
    #[arg(long)]
    language: Option<LanguageTag>,
    /// Manga reading direction: "Yes", "No", or "YesAndRightToLeft".
    #[arg(long)]
    manga: Option<Manga>,
    /// Summary/description for ComicInfo.xml metadata.
    #[arg(long)]
    summary: Option<String>,
    /// Directories to convert.
    path: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
enum Manga {
    Yes,
    No,
    YesAndRightToLeft,
}

impl FromStr for Manga {
    type Err = anyhow::Error;

    #[inline]
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "Yes" => Ok(Manga::Yes),
            "No" => Ok(Manga::No),
            "YesAndRightToLeft" => Ok(Manga::YesAndRightToLeft),
            _ => Err(anyhow!("Invalid manga value '{}'", s)),
        }
    }
}

impl fmt::Display for Manga {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Manga::Yes => write!(f, "Yes"),
            Manga::No => write!(f, "No"),
            Manga::YesAndRightToLeft => write!(f, "YesAndRightToLeft"),
        }
    }
}

enum To {
    First,
    Last,
    MostPages,
    Largest,
    Smallest,
    Index(usize),
    Regex(Regex),
}

impl To {
    /// Picks a book from the list according to the strategy.
    fn pick(&self, books: &[Rc<Book>]) -> Option<usize> {
        match *self {
            To::First if !books.is_empty() => Some(0),
            To::Last => books.len().checked_sub(1),
            To::MostPages => books
                .iter()
                .enumerate()
                .max_by_key(|(_, b)| b.pages.len())
                .map(|(i, _)| i),
            To::Largest => books
                .iter()
                .enumerate()
                .max_by_key(|(_, b)| b.bytes())
                .map(|(i, _)| i),
            To::Smallest => books
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| b.bytes())
                .map(|(i, _)| i),
            To::Index(n) if n < books.len() => Some(n),
            To::Regex(ref re) => books
                .iter()
                .enumerate()
                .find(|(_, book)| re.is_match(&book.name))
                .map(|(i, _)| i),
            _ => None,
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
            "most-pages" => Ok(To::MostPages),
            "largest" => Ok(To::Largest),
            "smallest" => Ok(To::Smallest),
            s => {
                if let Ok(n) = s.parse::<usize>() {
                    return Ok(To::Index(n));
                }

                let re = Regex::new(s).with_context(|| anyhow!("Parsing regex '{s}'"))?;
                Ok(To::Regex(re))
            }
        }
    }
}

impl fmt::Display for To {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            To::First => write!(f, "first"),
            To::Last => write!(f, "last"),
            To::MostPages => write!(f, "most-pages"),
            To::Largest => write!(f, "largest"),
            To::Smallest => write!(f, "smallest"),
            To::Index(n) => n.fmt(f),
            To::Regex(re) => re.fmt(f),
        }
    }
}

#[derive(Clone)]
enum From {
    Full,
    Single(u32),
    RangeInclusive(u32, u32),
    Range(u32, u32),
    RangeOpen(u32),
    RangeTo(u32),
    RangeToInclusive(u32),
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
            From::RangeTo(end) => (..end).contains(&number),
            From::RangeToInclusive(end) => (..=end).contains(&number),
        }
    }
}

impl FromStr for From {
    type Err = anyhow::Error;

    #[inline]
    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim();

        if let Some((from, to)) = s.split_once("..=") {
            let from = from.trim();

            if from.is_empty() {
                let to = to.trim().parse()?;
                return Ok(From::RangeToInclusive(to));
            }

            let from = from.parse()?;
            let to = to.trim().parse()?;
            return Ok(From::RangeInclusive(from, to));
        };

        if let Some((from, to)) = s.split_once("..") {
            let from = from.trim();
            let to = to.trim();

            if from.is_empty() {
                if to.is_empty() {
                    return Ok(From::Full);
                }

                let to = to.parse()?;
                return Ok(From::RangeTo(to));
            }

            let from = from.parse()?;

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
    fn pick(&self, catalog: &Catalog) -> Option<usize> {
        for m in &self.matches {
            if m.from.matches(catalog.number)
                && let Some(index) = m.to.pick(&catalog.books)
            {
                return Some(index);
            }
        }

        for what in &self.catch_all {
            if let Some(index) = what.pick(&catalog.books) {
                return Some(index);
            }
        }

        None
    }
}

/// Accepted image file extensions.
macro_rules! ext {
    () => {
        "jpg" | "png" | "gif" | "bmp" | "tif" | "webp" | "avif"
    };
}

/// Translates certain extensions to their more common forms.
fn translate(input: &str) -> &str {
    if input.eq_ignore_ascii_case("jpeg") {
        return "jpg";
    }

    if input.eq_ignore_ascii_case("tiff") {
        return "tif";
    }

    input
}

pub fn entry(opts: &Bookvert) -> Result<()> {
    let mut warn: ColorSpec = ColorSpec::new();
    warn.set_fg(Some(termcolor::Color::Yellow));

    let mut ok: ColorSpec = ColorSpec::new();
    ok.set_fg(Some(termcolor::Color::Green));

    let mut error: ColorSpec = ColorSpec::new();
    error.set_fg(Some(termcolor::Color::Red));

    let mut skip = Vec::<Regex>::new();
    let mut picker = Picker::default();

    for pat in &opts.pick {
        picker
            .parse(pat)
            .with_context(|| anyhow!("Parsing pick predicate '{}'", pat))?;
    }

    for pat in &opts.skip {
        let re = Regex::new(pat).with_context(|| anyhow!("Parsing regex '{}'", pat))?;
        skip.push(re);
    }

    let mut files = Vec::new();

    for path in &opts.path {
        for p in Walk::new(path) {
            let entry = p?;

            let Some(ty) = entry.file_type() else {
                continue;
            };

            if ty.is_file() {
                let path = entry.into_path();

                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(translate)
                    .map(|e| e.to_lowercase());

                let Some(ext) = ext else {
                    continue;
                };

                if !matches!(ext.as_str(), ext!()) {
                    continue;
                }

                files.push((path, ext));
            }
        }
    }

    files.sort();

    let o = StandardStream::stdout(termcolor::ColorChoice::Auto);
    let mut o = o.lock();

    let mut books_by_path = BTreeMap::<&Path, _>::new();
    let mut by_number = BTreeMap::<_, Vec<_>>::new();
    let mut state = State::default();

    for (from, ext) in &files {
        let Some(dir) = from.parent() else {
            continue;
        };

        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if skip.iter().any(|re| re.is_match(name)) {
            continue;
        }

        let book = books_by_path.entry(dir).or_insert_with(|| Book {
            dir: dir.to_path_buf(),
            name: name.to_string(),
            pages: Vec::new(),
            numbers: numbers(name).collect(),
        });

        book.pages.push(Page {
            path: from.to_owned(),
            name: format!("p{:03}.{ext}", book.pages.len()),
            metadata: fs::metadata(from)
                .with_context(|| anyhow!("{}: Failed to get metadata", from.display()))?,
        });
    }

    for (_, book) in books_by_path {
        let book = Rc::new(book);

        state.names.insert(book.name.clone());

        for &n in &book.numbers {
            by_number.entry(n).or_default().push(book.clone());
        }
    }

    for value in by_number.values_mut() {
        value.sort_by(|a, b| a.key().cmp(&b.key()));
    }

    if !opts.include.is_empty() {
        by_number.retain(|number, _| {
            opts.include
                .iter()
                .any(|predicate| predicate.matches(*number))
        });
    }

    for (number, books) in by_number {
        let mut catalog = Catalog {
            number,
            books,
            picked: None,
        };

        if catalog.books.len() == 1 {
            catalog.picked = Some(0);
        } else {
            catalog.picked = picker.pick(&catalog);
        }

        state.catalogs.push(catalog);
    }

    // Automatically determine name to use if possible.
    'name: {
        if let Some(name) = &opts.name {
            state.name = Some(name.clone());
            break 'name;
        }

        let mut it = state.names.iter();

        if let Some(first) = it.next()
            && it.next().is_none()
        {
            state.name = Some(first.to_string());
            break 'name;
        }
    }

    if opts.noninteractive {
        let mut is_error = false;

        if state.name.is_none() {
            o.set_color(&error)?;
            write!(o, "[error] ")?;
            o.reset()?;

            writeln!(o, "Use `--name <name>` to set one name of the series:")?;

            for name in &state.names {
                writeln!(o, "  {}", escape(name))?;
            }

            is_error = true;
        }

        for catalog in &state.catalogs {
            if catalog.picked.is_some() {
                continue;
            }

            o.set_color(&error)?;
            write!(o, "[error] ")?;
            o.reset()?;

            writeln!(
                o,
                "{number:03}: more than one match, use something like `-p {number}=0` to pick one:",
                number = catalog.number,
            )?;

            for (idx, book) in catalog.books.iter().enumerate() {
                writeln!(
                    o,
                    "  {idx}: {} ({} pages, {} bytes)",
                    escape(&book.name),
                    book.pages.len(),
                    book.bytes(),
                )?;

                if opts.verbose {
                    o.set_color(&warn)?;
                    write!(o, "    [source]")?;
                    o.reset()?;
                    writeln!(o, " {}", book.dir.display())?;
                }
            }

            is_error = true;
        }

        if is_error {
            return Err(anyhow!("Aborting due to non-interactive errors."));
        }
    } else {
        let mut app = App::default();

        if !app.run(&mut state)? {
            return Err(anyhow!("Aborting due to user cancellation."));
        }
    }

    let name = state.name.context("No name specified for catalog")?;

    for c in &state.catalogs {
        let Some(book) = c.selected() else {
            continue;
        };

        let mut target = opts.out.clone();
        target.push(format!("{name}{:03}", c.number));
        target.add_extension("cbz");

        let color = if opts.dry_run { &warn } else { &ok };
        o.set_color(color)?;
        write!(o, "[from]")?;
        o.reset()?;

        writeln!(o, " {:03}: {}", c.number, book.dir.display())?;

        let comic_info = config_info(opts, &name, c, book).context("ComicInfo.xml generation")?;

        if opts.verbose {
            o.set_color(&ok)?;
            write!(o, "  [info] ")?;
            o.reset()?;
            writeln!(o, "ComicInfo.xml:")?;

            for line in comic_info.lines() {
                writeln!(o, "    {line}")?;
            }
        }

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

        w.start_file("ComicInfo.xml", options)?;
        w.write_all(comic_info.as_bytes())?;

        for page in book.pages.iter() {
            let content = fs::read(&page.path)
                .with_context(|| anyhow!("Failed to read file {}", page.path.display()))?;

            w.start_file(&page.name, options)?;
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

/// Generates ComicInfo.xml content if any metadata options are provided.
fn config_info(opts: &Bookvert, name: &str, catalog: &Catalog, book: &Book) -> Result<String> {
    let mut o = String::new();

    writeln!(o, "<?xml version=\"1.0\" encoding=\"utf-8\"?>")?;
    writeln!(
        o,
        "<ComicInfo xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\">"
    )?;

    writeln!(
        o,
        "  <Title>{}</Title>",
        xml_escape(&format!("{name}{}", catalog.number))
    )?;

    let series = opts.series.as_deref().unwrap_or(name);
    writeln!(o, "  <Series>{}</Series>", xml_escape(series))?;
    writeln!(o, "  <Number>{}</Number>", catalog.number)?;
    writeln!(o, "  <PageCount>{}</PageCount>", book.pages.len())?;

    if let Some(author) = &opts.author {
        writeln!(o, "  <Writer>{}</Writer>", xml_escape(author))?;
    }

    if let Some(artist) = &opts.artist {
        writeln!(o, "  <Penciller>{}</Penciller>", xml_escape(artist))?;
    }

    if let Some(publisher) = &opts.publisher {
        writeln!(o, "  <Publisher>{}</Publisher>", xml_escape(publisher))?;
    }

    if let Some(genre) = &opts.genre {
        writeln!(o, "  <Genre>{}</Genre>", xml_escape(genre))?;
    }

    if let Some(language) = &opts.language {
        writeln!(o, "  <LanguageISO>{language}</LanguageISO>")?;
    }

    if let Some(manga) = &opts.manga {
        writeln!(o, "  <Manga>{manga}</Manga>")?;
    }

    if let Some(summary) = &opts.summary {
        writeln!(o, "  <Summary>{}</Summary>", xml_escape(summary))?;
    }

    writeln!(o, "</ComicInfo>")?;
    Ok(o)
}

/// Terminal escape.
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

/// Escapes special XML characters.
fn xml_escape(input: &str) -> Cow<'_, str> {
    let mut escaped = String::new();

    let n = 'escape: {
        for (n, c) in input.char_indices() {
            if !matches!(c, '&' | '<' | '>' | '"' | '\'') {
                continue;
            }

            break 'escape n;
        }

        return Cow::Borrowed(input);
    };

    escaped.push_str(&input[..n]);

    for c in input[n..].chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c => escaped.push(c),
        }
    }

    Cow::Owned(escaped)
}
