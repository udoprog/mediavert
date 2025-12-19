use core::cell::Cell;

use std::collections::HashSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use termcolor::{ColorChoice, StandardStream};

use crate::bitrates::Bitrates;
use crate::condition::{Condition, FromCondition, ToCondition};
use crate::config::Config;
use crate::format::Format;
use crate::out::{Colors, Out, blank, error, info, warn};
use crate::set_bit_rate::SetBitRate;
use crate::shell::{self, FormatCommand};
use crate::tasks::{MatchingConversion, TaskKind, Tasks, TransferKind, Trash, TrashWhat};

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
    let mut tasks = Tasks::new();

    config.populate(&mut tasks)?;

    for (path, ext) in tasks.unsupported_extensions.drain(..) {
        warn!(o, "Unsupported extension: {ext}");
        let mut o = o.indent(1);
        blank!(o, "Path: {}", shell::escape(path.as_os_str()));
    }

    for (from, to) in tasks.already_exists.drain(..) {
        warn!(o => v, "already exists (--force to remove):");
        let mut o = o.indent(1);
        blank!(o => v, "from : {}", shell::escape(from.as_os_str()));
        blank!(o => v, "to   : {}", shell::escape(to.as_os_str()));
    }

    for e in &tasks.errors {
        error!(o, "Error: {}", shell::escape(e.path.as_os_str()));
        let mut o = o.indent(1);

        for m in &e.messages {
            error!(o, "{m}");
        }
    }

    for d in &tasks.meta_dumps {
        d.dump(o)?;
    }

    if !tasks.errors.is_empty() && !config.keep_going {
        bail!("Aborting due to previous errors, use --keep-going to ignore.");
    }

    for MatchingConversion {
        from_path,
        from,
        to_formats,
    } in tasks.matching_conversions.drain(..)
    {
        let to_formats = to_formats
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        info!(o => v, "Found matching conversions: {from} -> {to_formats}");
        let mut o = o.indent(1);
        blank!(o => v, "path: {}", shell::escape(from_path.as_os_str()));
    }

    let total = tasks.tasks.len();

    for c in &mut tasks.tasks {
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

    let mut n = 0u32;

    for c in tasks.tasks.iter().filter(|c| c.is_completed()) {
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

        tasks.to_trash.push(Trash {
            what: TrashWhat::SourceFile,
            path: c.from_path.clone(),
            name: file_name.to_owned(),
        });
    }

    // Ensure trash directory exists.
    if !tasks.to_trash.is_empty() && !config.trash.is_dir() {
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
    for Trash { what, path, name } in tasks.to_trash.drain(..) {
        let trash_path = config.trash.join(&name);

        info!(o, "Trashing {what}");
        let mut o = o.indent(1);
        blank!(o, "from : {}", shell::escape(path.as_os_str()));
        blank!(o, "to   : {}", shell::escape(trash_path.as_os_str()));

        if !config.dry_run
            && let Err(e) = fs::rename(&path, &trash_path)
        {
            error!(o, "{e}");

            if let Some(path) = path.parent() {
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
