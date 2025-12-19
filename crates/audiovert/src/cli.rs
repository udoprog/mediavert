use core::cell::Cell;
use core::fmt;
use core::str::FromStr;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use termcolor::{ColorChoice, StandardStream};

use crate::meta;
use crate::out::{Colors, Out, blank, error, info, warn};
use crate::shell::{self, FormatCommand};

const DEFAULT_BITRATE_AAC: u32 = 192;
const DEFAULT_BITRATE_MP3: u32 = 320;
const DEFAULT_BITRATE_OGG: u32 = 192;

const DEFAULT_BITRATES: [(Format, u32); 3] = [
    (Format::Aac, DEFAULT_BITRATE_AAC),
    (Format::Mp3, DEFAULT_BITRATE_MP3),
    (Format::Ogg, DEFAULT_BITRATE_OGG),
];

const PART: &str = "part";

/// A tool to perform batch conversion of audio.
#[derive(Parser)]
pub struct Audiovert {
    /// If set, forces overwriting of existing files if a source file exists and
    /// the destination file also exists.
    #[arg(short = 'f', long)]
    force: bool,
    /// If set, enables verbose output.
    #[arg(short = 'v', long)]
    verbose: bool,
    /// Removed files will be moved to this location instead of being
    /// deleted [default: ~/trash].
    #[arg(long)]
    trash: Option<PathBuf>,
    /// If set, source files are trashed after successful conversion.
    #[arg(short = 'r', long)]
    trash_source: bool,
    /// Conversion pairs to perform, like flac=mp3 which would mean converting
    /// from flac to mp3. This also takes special values like lossless=<format>,
    /// lossy=<format> or same.
    ///
    /// The target <format> can also specify an exact format or the special
    /// keyword same. With this a flexible rules of conversions can be defined.
    ///
    /// By default, conversions are performed from lossless formats to mp3, and
    /// to link lossy formats.
    ///
    /// Note that multiple matching conversions can be specified, in which case
    /// multiple target files will be produced.
    #[arg(short = 'c', long)]
    conversion: Vec<Condition>,
    /// If set, performs a dry run without making any changes. This also implies
    /// verbose.
    #[arg(short = 'D', long)]
    dry_run: bool,
    /// If set, continues processing files even if errors are encountered.
    #[arg(short = 'k', long)]
    keep_going: bool,
    /// Output base directory for converted files.
    #[arg(short = 'o', long)]
    to: Option<PathBuf>,
    /// If set, uses metadata to determine the output path. This can only makes
    /// sense when used with `--to`. If all the required metadata are missing,
    /// the file will be ignored.
    ///
    /// The output path will be:
    ///
    /// {Artist} / {Album} ({Year}) / {Artist} - {Track Number} - {Title}.{ext}
    #[arg(long)]
    meta: bool,
    /// If set, dumps metadata for each file processed with `--meta`.
    #[arg(long)]
    meta_dump: bool,
    /// If set, dumps metadata for each file processed with `--meta` that has
    /// errors.
    #[arg(long)]
    meta_dump_error: bool,
    /// If set, moves files instead of creating hard links when transferring.
    #[arg(long)]
    r#move: bool,
    /// Bitrates to use when performing conversions. This has the format
    /// <format>=<number> where <number> is the desired bitrate in kbps. If 0 is
    /// set, then the default bitrate for that format is used.
    ///
    /// Default bitrates are 320kbps for mp3 and 192kbps for ogg and aac.
    #[arg(long)]
    bitrates: Vec<SetBitRate>,
    /// If set, forces re-encoding of the formats specified in --bitrates.
    #[arg(long)]
    force_bitrates: bool,
    /// Path to ffmpeg binary to use when performing conversions.
    #[arg(long, default_value = "ffmpeg")]
    ffmpeg_bin: PathBuf,
    /// The extension to use for partial conversion files.
    ///
    /// These are used in place of the target file during conversion, and
    /// renamed once conversion has been verified to be successful.
    ///
    /// If these files are encountered during future conversions, they will be
    /// removed.
    #[arg(long, default_value = PART)]
    part_ext: String,
    /// Paths to process.
    paths: Vec<PathBuf>,
}

/// Configuration for conversions.
struct Config {
    bitrates: Bitrates,
    conversion: Vec<Condition>,
    dry_run: bool,
    ffmpeg: PathBuf,
    force: bool,
    keep_going: bool,
    meta_dump_error: bool,
    meta_dump: bool,
    meta: bool,
    part_ext: String,
    paths: Vec<PathBuf>,
    r#move: bool,
    forced_bitrates: HashSet<Format>,
    to_dir: Option<PathBuf>,
    trash_source: bool,
    trash: PathBuf,
    verbose: bool,
}

impl Config {
    fn make_dir(&self, o: &mut Out<'_>, what: impl fmt::Display, path: &Path) -> Result<bool> {
        let Some(parent) = path.parent() else {
            return Ok(true);
        };

        if parent.is_dir() {
            return Ok(true);
        }

        info!(o, "making {what} dir");
        let mut o = o.indent(1);
        blank!(o, "mkdir -p {}", shell::escape(parent.as_os_str()));

        if self.dry_run {
            return Ok(true);
        }

        if let Err(e) = fs::create_dir_all(parent) {
            error!(o, "{e}");
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

/// Entry for `audiovert`.
///
/// See [`crate`] documentation.
pub fn entry(opts: &Audiovert) -> Result<()> {
    // Current indentation level for output.
    let indent = Cell::new(0);
    // Collection of bitrates.
    let mut bitrates = Bitrates::default();
    // Formats to re-encode.
    let mut forced_bitrates = HashSet::new();

    for bitrate in &opts.bitrates {
        for (format, to) in bitrate.from.pick_bitrates(&mut bitrates) {
            let Some(default_bitrate) = format.default_bitrate() else {
                bail!("Cannot set custom bitrate for format: {format}");
            };

            if opts.force_bitrates {
                forced_bitrates.insert(format);
            }

            *to = if bitrate.bitrate == 0 {
                default_bitrate
            } else {
                bitrate.bitrate
            };
        }
    }

    let trash = match &opts.trash {
        Some(p) => p.clone(),
        None => 'trash: {
            let mut trash = env::home_dir().context("Get home directory")?;

            for d in ["trash", "Trash"] {
                trash.push(d);

                if trash.is_dir() {
                    break 'trash trash;
                }

                trash.pop();
            }

            trash.push("trash");
            trash
        }
    };

    let mut config = Config {
        bitrates,
        conversion: opts.conversion.clone(),
        dry_run: opts.dry_run,
        ffmpeg: opts.ffmpeg_bin.clone(),
        force: opts.force,
        keep_going: opts.keep_going,
        meta_dump_error: opts.meta_dump_error,
        meta_dump: opts.meta_dump,
        meta: opts.meta,
        part_ext: opts.part_ext.clone(),
        paths: opts.paths.clone(),
        r#move: opts.r#move,
        forced_bitrates,
        to_dir: opts.to.clone(),
        trash_source: opts.trash_source,
        trash,
        verbose: opts.verbose,
    };

    if config.paths.is_empty() {
        config.paths.push(PathBuf::from("."));
    }

    if config.conversion.is_empty() {
        config.conversion.push(Condition::FromTo {
            from: FromCondition::Lossless,
            to: ToCondition::Exact(Format::Mp3),
        });

        config.conversion.push(Condition::FromTo {
            from: FromCondition::Lossy,
            to: ToCondition::Same,
        });
    }

    let cols = Colors::new();

    let o = StandardStream::stdout(ColorChoice::Auto);
    let mut o = o.lock();
    let mut o = Out::new(config.verbose, &indent, &cols, &mut o);
    run(&mut o, &config)
}

fn run(o: &mut Out<'_>, config: &Config) -> Result<()> {
    let mut index = 0u32;

    let mut tag_errors = Vec::new();
    let mut tag_items = Vec::new();

    let mut meta_dumps = Vec::new();
    let mut errors = Vec::new();
    let mut matching_conversions = Vec::new();
    let mut tasks = Vec::new();

    for path in &config.paths {
        for f in ignore::Walk::new(path) {
            let entry = f?;

            let from_path = entry.path();

            if !from_path.is_file() {
                continue;
            }

            let Some(ext) = from_path.extension().and_then(|s| s.to_str()) else {
                continue;
            };

            let Some(from) = Format::from_ext(ext) else {
                warn!(o, "Unsupported extension: {ext}");
                let mut o = o.indent(1);
                blank!(o, "Path: {}", shell::escape(from_path.as_os_str()));
                continue;
            };

            let mut to_formats = BTreeSet::new();

            for conversion in &config.conversion {
                to_formats.extend(conversion.to_format(from));
            }

            if !to_formats.is_empty() && config.verbose {
                matching_conversions.push((from_path.to_path_buf(), from, to_formats.clone()));
            }

            let id_parts = if config.meta {
                let id_parts = meta::Parts::from_path(
                    from_path,
                    &mut tag_errors,
                    (config.meta_dump || config.meta_dump_error).then_some(&mut tag_items),
                );

                let has_errors = !tag_errors.is_empty();

                if !tag_errors.is_empty() {
                    errors.push(Error {
                        path: from_path.to_path_buf(),
                        messages: tag_errors.drain(..).collect(),
                    });
                }

                if !tag_items.is_empty() {
                    if config.meta_dump || (config.meta_dump_error && has_errors) {
                        meta_dumps.push(meta::Dump {
                            path: from_path.to_path_buf(),
                            items: tag_items.drain(..).collect(),
                        });
                    }

                    tag_items.clear();
                }

                id_parts
            } else {
                None
            };

            for to in to_formats {
                let mut pre_remove = Vec::new();

                let to_path = if let Some(to_dir) = &config.to_dir {
                    match &id_parts {
                        Some(id_parts) => {
                            let mut to_path = to_dir.to_path_buf();
                            id_parts.append_to(&mut to_path);
                            to_path.add_extension(to.ext());
                            to_path
                        }
                        None => {
                            let Some(suffix) = from_path.strip_prefix(path).ok() else {
                                errors.push(Error {
                                    path: from_path.to_path_buf(),
                                    messages: vec!["Failed to get suffix for path".to_string()],
                                });

                                continue;
                            };

                            let mut to_path = to_dir.join(suffix);
                            to_path.set_extension(to.ext());
                            to_path
                        }
                    }
                } else {
                    match &id_parts {
                        Some(id_parts) => {
                            let mut to_path = path.to_path_buf();
                            id_parts.append_to(&mut to_path);
                            to_path.add_extension(to.ext());
                            to_path
                        }
                        None => {
                            let mut to_path = from_path.to_path_buf();
                            to_path.set_extension(to.ext());
                            to_path
                        }
                    }
                };

                if from_path == to_path {
                    continue;
                }

                let exists = if to_path.exists() {
                    if !config.force {
                        warn!(o => v, "already exists (--force to remove):");
                        let mut o = o.indent(1);
                        blank!(o => v, "from : {}", shell::escape(from_path.as_os_str()));
                        blank!(o => v, "to   : {}", shell::escape(to_path.as_os_str()));
                        true
                    } else {
                        pre_remove.push(("destination path (--force)", to_path.clone()));
                        false
                    }
                } else {
                    false
                };

                let kind = if from == to && !config.forced_bitrates.contains(&from) {
                    TaskKind::Transfer {
                        kind: if config.r#move {
                            TransferKind::Move
                        } else {
                            TransferKind::Link
                        },
                    }
                } else {
                    let part_path = to_path.with_added_extension(&config.part_ext);

                    if part_path.exists() {
                        pre_remove.push(("partial conversion file", part_path.clone()));
                    }

                    TaskKind::Convert {
                        part_path,
                        from,
                        to,
                        converted: exists,
                    }
                };

                tasks.push(Task {
                    index,
                    kind,
                    from_path: from_path.to_path_buf(),
                    to_path,
                    moved: exists,
                    pre_remove,
                });

                index = index.saturating_add(1);
            }
        }
    }

    for e in &errors {
        error!(o, "Error: {}", shell::escape(e.path.as_os_str()));
        let mut o = o.indent(1);

        for m in &e.messages {
            error!(o, "{m}");
        }
    }

    for d in &meta_dumps {
        d.dump(o)?;
    }

    if !errors.is_empty() && !config.keep_going {
        bail!("Aborting due to previous errors, use --keep-going to ignore.");
    }

    for (from_path, from, to_formats) in matching_conversions {
        let to_formats = to_formats
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        info!(o => v, "Found matching conversions: {from} -> {to_formats}");
        let mut o = o.indent(1);
        blank!(o => v, "path: {}", shell::escape(from_path.as_os_str()));
    }

    let total = index;

    for c in &mut tasks {
        if c.is_completed() {
            continue;
        }

        info!(o, "Task #{}/#{total}: {}", c.index, c.kind);
        let mut o = o.indent(1);

        blank!(o, "from : {}", shell::escape(c.from_path.as_os_str()));
        blank!(o, "to   : {}", shell::escape(c.to_path.as_os_str()));

        for (reason, path) in c.pre_remove.drain(..) {
            info!(o, "removing {reason}");
            let mut o = o.indent(1);

            if config.verbose {
                blank!(o, "rm {}", shell::escape(path.as_os_str()));
            } else {
                blank!(o, "rm <to>.{}", config.part_ext);
            }

            if !config.dry_run
                && let Err(e) = fs::remove_file(&path)
            {
                error!(o, "{e}");
            }
        }

        match c.kind {
            TaskKind::Convert {
                ref part_path,
                to,
                ref mut converted,
                ..
            } => {
                if !*converted {
                    let mut cmd = Command::new(&config.ffmpeg);
                    cmd.args(["-hide_banner", "-loglevel", "error"]);
                    cmd.args([OsStr::new("-i"), c.from_path.as_os_str()]);
                    to.bitrate(config, &mut cmd);
                    cmd.args(["-map_metadata", "0", "-id3v2_version", "3"]);
                    cmd.args(["-f", to.ffmpeg_format()]);
                    cmd.arg(part_path);

                    let mut f = FormatCommand::new(&cmd);

                    if !config.verbose {
                        f.insert_replacement(config.ffmpeg.as_os_str(), "<ffmpeg>");
                        f.insert_replacement(c.from_path.as_os_str(), "<from>");
                        f.insert_replacement(
                            part_path.as_os_str(),
                            format!("<to>.{}", config.part_ext),
                        );
                    }

                    if !config.make_dir(&mut o, "partial", part_path)? {
                        continue;
                    }

                    {
                        blank!(o, "{f}");
                        let mut o = o.indent(1);

                        if !config.dry_run {
                            let status = match cmd.status() {
                                Ok(s) => s,
                                Err(e) => {
                                    error!(o, "{e}");
                                    continue;
                                }
                            };

                            *converted = status.success();
                        } else {
                            *converted = true;
                        }
                    }

                    if *converted && !c.moved {
                        if !config.make_dir(&mut o, "rename", &c.to_path)? {
                            continue;
                        }

                        blank!(o, "mv <to>.{} <to>", config.part_ext);
                        let mut o = o.indent(1);

                        if config.verbose {
                            blank!(o, "from : {}", shell::escape(part_path.as_os_str()));
                            blank!(o, "to   : {}", shell::escape(c.to_path.as_os_str()));
                        }

                        if !config.dry_run {
                            if let Err(e) = fs::rename(part_path, &c.to_path) {
                                error!(o, "{e}");
                            } else {
                                c.moved = true;
                            }
                        } else {
                            c.moved = true;
                        }
                    }
                }
            }
            TaskKind::Transfer { kind } => {
                if !c.moved {
                    if !config.make_dir(&mut o, kind, &c.to_path)? {
                        continue;
                    }

                    if config.verbose {
                        blank!(o, "from : {}", shell::escape(c.from_path.as_os_str()));
                        blank!(o, "to   : {}", shell::escape(c.to_path.as_os_str()));
                    } else {
                        blank!(o, "{} <from> <to>", kind.symbolic_command());
                    }

                    if !config.dry_run {
                        let result = match kind {
                            TransferKind::Link => fs::hard_link(&c.from_path, &c.to_path),
                            TransferKind::Move => fs::rename(&c.from_path, &c.to_path),
                        };

                        if let Err(e) = result {
                            error!(o, "{e}");
                        } else {
                            c.moved = true;
                        }
                    } else {
                        c.moved = true;
                    }
                }
            }
        }
    }

    let mut to_trash = Vec::new();
    let mut n = 0u32;

    for c in tasks.iter().filter(|c| c.is_completed()) {
        if !config.trash_source {
            continue;
        }

        // NB: Trashing is meaningless for moved files.
        if matches!(
            c.kind,
            TaskKind::Transfer {
                kind: TransferKind::Move
            }
        ) {
            continue;
        }

        let new;

        let file_name = match c.from_path.file_name() {
            Some(name) => name,
            None => {
                new = OsString::from(format!("file{}", n));
                n += 1;
                &new
            }
        };

        let trash_path = config.trash.join(file_name);
        to_trash.push(("source file", c.from_path.clone(), trash_path));
    }

    // Ensure trash directory exists.
    if !to_trash.is_empty() && !config.trash.is_dir() {
        info!(o, "Creating trash directory");

        let mut o = o.indent(1);

        blank!(o => v, "path: {}", shell::escape(config.trash.as_os_str()));

        if !config.dry_run
            && let Err(e) = fs::create_dir_all(&config.trash)
        {
            error!(o, "{e}");
        }
    }

    let mut check_empty = Vec::new();

    // Move files to trash.
    for (what, from, trash_path) in to_trash {
        info!(o, "Trashing {what}");
        let mut o = o.indent(1);
        blank!(o, "from : {}", shell::escape(from.as_os_str()));
        blank!(o, "to   : {}", shell::escape(trash_path.as_os_str()));

        if !config.dry_run
            && let Err(e) = fs::rename(&from, &trash_path)
        {
            error!(o, "{e}");

            if let Some(path) = from.parent() {
                check_empty.push(path.to_path_buf());
            }
        }
    }

    // Recursively check for empty directories and remove them.
    for mut path in check_empty {
        if !is_empty_dir(&path) {
            continue;
        }

        info!(o, "removing empty directory:");
        let mut o = o.indent(1);
        blank!(o, "path: {}", shell::escape(path.as_os_str()));

        if !config.dry_run {
            if let Err(e) = fs::remove_dir(&path) {
                error!(o, "{e}");
            }

            path.pop();
        } else {
            continue;
        }
    }

    Ok(())
}

fn is_empty_dir(path: &PathBuf) -> bool {
    let Ok(mut entries) = fs::read_dir(path) else {
        return false;
    };

    entries.next().is_none()
}

#[derive(Clone, Copy)]
struct SetBitRate {
    from: FromCondition,
    bitrate: u32,
}

impl FromStr for SetBitRate {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (format, bitrate) = s.split_once('=').ok_or("missing '=' separator")?;
        let format = FromCondition::from_str(format)?;
        let bitrate = bitrate.parse::<u32>().map_err(|_| "invalid bitrate")?;
        Ok(SetBitRate {
            from: format,
            bitrate,
        })
    }
}

impl fmt::Display for SetBitRate {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.from, self.bitrate)
    }
}

struct Bitrates {
    map: HashMap<Format, u32>,
}

impl Default for Bitrates {
    #[inline]
    fn default() -> Self {
        Self {
            map: HashMap::from(DEFAULT_BITRATES),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum FromCondition {
    Lossless,
    Lossy,
    Exact(Format),
}

impl FromCondition {
    fn pick_bitrates<'bits>(
        &self,
        bitrates: &'bits mut Bitrates,
    ) -> impl Iterator<Item = (Format, &'bits mut u32)> + 'bits {
        let this = *self;
        bitrates
            .map
            .iter_mut()
            .filter(move |(format, _)| this.matches(**format))
            .map(|(f, v)| (*f, v))
    }

    fn matches(self, format: Format) -> bool {
        match self {
            FromCondition::Lossless => format.is_lossless(),
            FromCondition::Lossy => !format.is_lossless(),
            FromCondition::Exact(f) => f == format,
        }
    }
}

impl FromStr for FromCondition {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "lossless" => Ok(Self::Lossless),
            "lossy" => Ok(Self::Lossy),
            _ => Ok(Self::Exact(Format::from_str(s)?)),
        }
    }
}

impl fmt::Display for FromCondition {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FromCondition::Lossless => write!(f, "lossless"),
            FromCondition::Lossy => write!(f, "lossy"),
            FromCondition::Exact(format) => format.fmt(f),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum ToCondition {
    Exact(Format),
    Same,
}

impl ToCondition {
    fn to_format(self, format: Format) -> Format {
        match self {
            ToCondition::Exact(f) => f,
            ToCondition::Same => format,
        }
    }
}

impl FromStr for ToCondition {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "same" {
            Ok(ToCondition::Same)
        } else {
            let format = Format::from_str(s)?;
            Ok(ToCondition::Exact(format))
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum Condition {
    FromTo {
        from: FromCondition,
        to: ToCondition,
    },
    To {
        to: ToCondition,
    },
    Same,
}

impl Condition {
    fn to_format(self, format: Format) -> Option<Format> {
        match self {
            Condition::Same => Some(format),
            Condition::To { to } => Some(to.to_format(format)),
            Condition::FromTo { from, to } => {
                if from.matches(format) {
                    Some(to.to_format(format))
                } else {
                    None
                }
            }
        }
    }
}

impl FromStr for Condition {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "same" => Ok(Condition::Same),
            _ => {
                let Some((from_str, to_str)) = s.split_once('=') else {
                    let to = ToCondition::from_str(s)?;
                    return Ok(Condition::To { to });
                };

                let from = FromCondition::from_str(from_str)?;
                let to = ToCondition::from_str(to_str)?;
                Ok(Condition::FromTo { from, to })
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Format {
    Aac,
    Flac,
    Mp3,
    Ogg,
    Wav,
}

impl Format {
    fn default_bitrate(&self) -> Option<u32> {
        match self {
            Format::Aac => Some(DEFAULT_BITRATE_AAC),
            Format::Mp3 => Some(DEFAULT_BITRATE_MP3),
            Format::Ogg => Some(DEFAULT_BITRATE_OGG),
            _ => None,
        }
    }

    fn is_lossless(&self) -> bool {
        matches!(self, Format::Flac | Format::Wav)
    }

    fn bitrate(&self, config: &Config, command: &mut Command) {
        if let Some(bitrate) = config.bitrates.map.get(self)
            && *bitrate > 0
        {
            command.arg("-ab");
            command.arg(format!("{}k", bitrate));
        }
    }

    fn ext(&self) -> &'static str {
        match self {
            Format::Aac => "aac",
            Format::Flac => "flac",
            Format::Mp3 => "mp3",
            Format::Ogg => "ogg",
            Format::Wav => "wav",
        }
    }

    fn ffmpeg_format(&self) -> &'static str {
        match self {
            Format::Aac => "adts",
            Format::Flac => "flac",
            Format::Mp3 => "mp3",
            Format::Ogg => "ogg",
            Format::Wav => "wav",
        }
    }

    fn from_ext(ext: &str) -> Option<Format> {
        match ext {
            "aac" => Some(Format::Aac),
            "flac" => Some(Format::Flac),
            "mp3" => Some(Format::Mp3),
            "ogg" => Some(Format::Ogg),
            "wav" => Some(Format::Wav),
            _ => None,
        }
    }
}

impl fmt::Display for Format {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ext().fmt(f)
    }
}

impl FromStr for Format {
    type Err = &'static str;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_ext(s).ok_or("unsupported format")
    }
}

#[derive(Debug, Clone, Copy)]
enum TransferKind {
    Link,
    Move,
}

impl TransferKind {
    fn symbolic_command(&self) -> &'static str {
        match self {
            TransferKind::Link => "ln",
            TransferKind::Move => "mv",
        }
    }
}

impl fmt::Display for TransferKind {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransferKind::Link => write!(f, "link"),
            TransferKind::Move => write!(f, "move"),
        }
    }
}

enum TaskKind {
    /// Convert from one format to another.
    Convert {
        part_path: PathBuf,
        from: Format,
        to: Format,
        converted: bool,
    },
    /// Transfer from source to destination.
    Transfer { kind: TransferKind },
}

impl TaskKind {
    fn is_completed(&self) -> bool {
        match self {
            TaskKind::Convert { converted, .. } => *converted,
            TaskKind::Transfer { .. } => true,
        }
    }
}

impl fmt::Display for TaskKind {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskKind::Convert { from, to, .. } => write!(f, "converting {} to {}", from, to),
            TaskKind::Transfer { kind } => kind.fmt(f),
        }
    }
}

struct Error {
    path: PathBuf,
    messages: Vec<String>,
}

struct Task {
    index: u32,
    kind: TaskKind,
    from_path: PathBuf,
    to_path: PathBuf,
    moved: bool,
    pre_remove: Vec<(&'static str, PathBuf)>,
}

impl Task {
    fn is_completed(&self) -> bool {
        self.kind.is_completed() && self.moved && self.pre_remove.is_empty()
    }
}
