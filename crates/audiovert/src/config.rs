use core::fmt;

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use relative_path::{Component, RelativePath, RelativePathBuf};

use crate::archive::Archive;
use crate::bitrates::Bitrates;
use crate::condition::Condition;
use crate::format::Format;
use crate::link::{Link, Linkable, MaybeLink};
use crate::meta;
use crate::out::{Out, blank, error, info};
use crate::shell;
use crate::tasks::{
    Exists, MatchingConversion, PathError, Task, TaskKind, Tasks, TransferKind, Unsupported,
};

/// Configuration for conversions.
pub(crate) struct Config {
    pub(crate) bitrates: Bitrates,
    pub(crate) conversion: Vec<Condition>,
    pub(crate) dry_run: bool,
    pub(crate) ffmpeg: PathBuf,
    pub(crate) force: bool,
    pub(crate) keep_going: bool,
    pub(crate) meta_dump_error: bool,
    pub(crate) meta_dump: bool,
    pub(crate) meta: bool,
    pub(crate) part_ext: String,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) r#move: bool,
    pub(crate) forced_bitrates: HashSet<Format>,
    pub(crate) to_dir: Option<PathBuf>,
    pub(crate) trash_source: bool,
    pub(crate) trash: PathBuf,
    pub(crate) verbose: bool,
}

impl Config {
    /// Populate tasks based on configuration.
    pub(crate) fn populate(&self, tasks: &mut Tasks) -> Result<()> {
        let mut tag_errors = Vec::new();
        let mut tag_items = Vec::new();
        let mut to_formats = BTreeSet::new();
        let mut sources = Vec::new();
        let mut pre_remove = Vec::new();

        for walk_path in &self.paths {
            let dir = if walk_path.is_file() {
                let Some(dir) = walk_path.parent() else {
                    // This only happens for empty arguments, so they should
                    // subsequently be ignored.
                    continue;
                };

                dir
            } else {
                walk_path
            };

            for f in ignore::Walk::new(walk_path) {
                let entry = f?;

                let walked = entry.path();

                if !walked.is_file() {
                    continue;
                }

                let Some(ext) = walked.extension().and_then(|s| s.to_str()) else {
                    continue;
                };

                if let Some(kind) = Archive::from_ext(ext) {
                    let archive_id = tasks.db.push_archive(SourceArchive {
                        kind,
                        path: Link::new(walked)?,
                    });

                    let mut archive_path = walked.parent().unwrap_or(Path::new("")).to_path_buf();

                    if let Some(file_name) = walked.file_stem() {
                        archive_path.push(file_name);
                    }

                    kind.enumerate(walked, &mut |path| {
                        let path = RelativePath::new(path);
                        let mut buf = archive_path.clone();

                        let ok = 'ok: {
                            for c in path.components() {
                                match c {
                                    Component::CurDir => {}
                                    Component::ParentDir => {
                                        break 'ok false;
                                    }
                                    Component::Normal(s) => {
                                        buf.push(s);
                                    }
                                }
                            }

                            true
                        };

                        if ok {
                            sources.push(Source::Archive {
                                archive: archive_id,
                                path: path.to_owned(),
                            });
                        }

                        Ok(())
                    })?;
                } else {
                    let file = tasks.db.push_file(Link::new(walked)?);
                    let source = Source::File { file };
                    sources.push(source);
                }

                for source in sources.drain(..) {
                    let Some(from) = tasks.db.ext(&source)?.and_then(Format::from_ext) else {
                        tasks.unsupported.push(Unsupported {
                            source,
                            ext: ext.to_string(),
                        });

                        continue;
                    };

                    to_formats.clear();

                    for conversion in &self.conversion {
                        to_formats.extend(conversion.to_format(from));
                    }

                    if !to_formats.is_empty() && self.verbose {
                        tasks.matching_conversions.push(MatchingConversion {
                            source: source.clone(),
                            from,
                            to_formats: to_formats.iter().cloned().collect(),
                        });
                    }

                    debug_assert!(tag_items.is_empty());

                    let id_parts = meta::Parts::from_path(
                        &source,
                        &tasks.db,
                        &mut tag_errors,
                        &mut tag_items,
                    )?;

                    if !tag_items.is_empty() {
                        let meta = tag_items.drain(..).collect();
                        tasks.meta.insert(source.clone(), meta);
                    }

                    let meta_parts = if self.meta {
                        let Some(id_parts) = id_parts else {
                            tag_errors.push(format!(
                                "could not extract required tags (see --meta-dump-error)"
                            ));
                            continue;
                        };

                        if !tag_errors.is_empty() {
                            tasks.errors.push(PathError {
                                source: source.clone(),
                                messages: tag_errors.drain(..).collect(),
                            });
                        }

                        Some(id_parts)
                    } else {
                        None
                    };

                    for &to in &to_formats {
                        debug_assert!(pre_remove.is_empty());

                        let to_path = if let Some(to_dir) = &self.to_dir {
                            match &meta_parts {
                                Some(meta_parts) => {
                                    let mut to_path = to_dir.to_path_buf();
                                    meta_parts.append_to(&mut to_path);
                                    to_path.add_extension(to.ext());
                                    to_path
                                }
                                None => {
                                    let mut to_path = to_dir.clone();
                                    tasks.db.to_dir_path(&source, dir, &mut to_path)?;
                                    to_path.set_extension(to.ext());
                                    to_path
                                }
                            }
                        } else {
                            match &meta_parts {
                                Some(meta_parts) => {
                                    let mut to_path = dir.to_path_buf();
                                    meta_parts.append_to(&mut to_path);
                                    to_path.add_extension(to.ext());
                                    to_path
                                }
                                None => {
                                    let mut to_path = tasks.db.to_path(&source)?;
                                    to_path.set_extension(to.ext());
                                    to_path
                                }
                            }
                        };

                        if tasks.db.as_file(&source)?.is_some_and(|p| p == to_path) {
                            continue;
                        }

                        let to_path = MaybeLink::new(to_path);
                        let exists;

                        if to_path.exists() {
                            if !self.force {
                                tasks.already_exists.push(Exists {
                                    source: source.clone(),
                                    path: Link::new(&to_path)?,
                                });
                                exists = true;
                            } else {
                                pre_remove.push(("destination path (--force)", to_path.clone()));
                                exists = false;
                            }
                        } else {
                            exists = false;
                        };

                        let kind = if from == to && !self.forced_bitrates.contains(&from) {
                            TaskKind::Transfer {
                                kind: match source {
                                    Source::File { .. } => {
                                        if self.r#move {
                                            TransferKind::Move
                                        } else {
                                            TransferKind::Link
                                        }
                                    }
                                    Source::Archive { .. } => TransferKind::Copy,
                                },
                            }
                        } else {
                            let part_path =
                                MaybeLink::new(to_path.with_added_extension(&self.part_ext));

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

                        let index = tasks.tasks.len();

                        tasks.tasks.push(Task {
                            index,
                            kind,
                            source: source.clone(),
                            to_path,
                            moved: exists,
                            pre_remove: pre_remove.drain(..).collect(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Make directory for output file.
    pub(crate) fn make_dir(
        &self,
        o: &mut Out<'_>,
        what: impl fmt::Display,
        path: &Path,
    ) -> Result<bool> {
        let Some(parent) = path.parent() else {
            return Ok(true);
        };

        if parent.components().next().is_none() || parent.is_dir() {
            return Ok(true);
        }

        info!(o, "making {what} dir");
        let mut o = o.indent(1);
        blank!(o, "mkdir -p {}", shell::path(parent));

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

/// The location and characteristics of a source archive.
///
/// This is referenced by an [`ArchiveId`].
#[derive(Clone)]
pub(crate) struct SourceArchive {
    /// Kind of the archive.
    pub(crate) kind: Archive,
    /// Path to the archive.
    pub(crate) path: Link,
}

impl SourceArchive {
    /// Get the contents of a file inside the archive.
    pub(crate) fn contents(&self, path: &RelativePath) -> Result<Vec<u8>> {
        if let Some(contents) = self.kind.contents(&self.path, path)? {
            return Ok(contents);
        }

        Err(anyhow!(
            "not found in archive: {}: {path}",
            self.path.display()
        ))
    }
}

/// Unique and internal identifier for a source file.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileId(usize);

impl fmt::Display for FileId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Unique and internal identifier for a source archive.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ArchiveId(usize);

impl fmt::Display for ArchiveId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Data associated with tasks.
pub(crate) struct Db {
    archives: Vec<SourceArchive>,
    files: Vec<Link>,
}

impl Db {
    /// Construct a new collection of archives.
    pub(crate) fn new() -> Self {
        Db {
            archives: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Get a file by its identifier.
    #[inline]
    pub(crate) fn file(&self, id: FileId) -> Result<&Link> {
        let Some(file) = self.files.get(id.0) else {
            return Err(anyhow!("invalid file id: {id}"));
        };

        Ok(file)
    }

    /// Get an archive by its identifier.
    #[inline]
    pub(crate) fn archive(&self, id: ArchiveId) -> Result<&SourceArchive> {
        let Some(archive) = self.archives.get(id.0) else {
            return Err(anyhow!("invalid archive id: {id}"));
        };

        Ok(archive)
    }

    /// Push a file to the collection.
    #[inline]
    pub(crate) fn push_file(&mut self, file: Link) -> FileId {
        let id = FileId(self.files.len());
        self.files.push(file);
        id
    }

    /// Push an archive to the collection.
    #[inline]
    pub(crate) fn push_archive(&mut self, archive: SourceArchive) -> ArchiveId {
        let id = ArchiveId(self.archives.len());
        self.archives.push(archive);
        id
    }

    /// Get the contents of the source file.
    pub(crate) fn archive_contents(
        &self,
        archive: ArchiveId,
        path: &RelativePath,
    ) -> Result<Vec<u8>> {
        let Some(archive) = self.archives.get(archive.0) else {
            anyhow::bail!("invalid archive id: {archive}");
        };

        archive.contents(path)
    }

    /// Append the relative source path to the given path.
    pub(crate) fn to_dir_path(
        &self,
        source: &Source,
        base: &Path,
        to_path: &mut PathBuf,
    ) -> Result<()> {
        match source {
            Source::File { file } => {
                let file = self.file(*file)?;

                let Ok(suffix) = file.strip_prefix(base) else {
                    bail!("invalid base path");
                };

                to_path.push(suffix);
            }
            Source::Archive { archive, path } => {
                let archive = self.archive(*archive)?;

                let Ok(suffix) = archive.path.strip_prefix(base) else {
                    bail!("invalid base path");
                };

                if let Some(parent) = suffix.parent() {
                    to_path.push(parent);
                }

                if let Some(file_stem) = archive.path.file_stem() {
                    to_path.push(file_stem);
                }

                for c in path.components() {
                    match c {
                        Component::CurDir => {}
                        Component::ParentDir => {
                            panic!("invalid path in archive: {path}");
                        }
                        Component::Normal(s) => {
                            to_path.push(s);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert an in-place source path to a regular filesystem path.
    pub(crate) fn to_path(&self, source: &Source) -> Result<PathBuf> {
        match source {
            Source::File { file } => {
                let file = self.file(*file)?;
                Ok(file.path().to_owned())
            }
            Source::Archive { archive, path } => {
                let archive = self.archive(*archive).context("no archive directory")?;

                let mut to_path = archive.path.path().to_owned();

                to_path.pop();

                if let Some(stem) = archive.path.file_stem() {
                    to_path.push(stem);
                }

                for c in path.components() {
                    match c {
                        Component::CurDir => {}
                        Component::ParentDir => {
                            panic!("invalid path in archive: {path}");
                        }
                        Component::Normal(s) => {
                            to_path.push(s);
                        }
                    }
                }

                Ok(to_path)
            }
        }
    }

    pub(crate) fn move_to(&self, source: &Source, to: &Path, kind: TransferKind) -> Result<()> {
        match source {
            Source::Archive { archive, path } => match kind {
                TransferKind::Link => bail!("cannot link from archive"),
                TransferKind::Move => bail!("cannot move from archive"),
                TransferKind::Copy => {
                    let contents = self.archive_contents(*archive, path)?;
                    fs::write(to, contents).context("writing file")?;
                }
            },
            Source::File { file } => {
                let file = self.file(*file)?;

                match kind {
                    TransferKind::Link => {
                        fs::hard_link(file, to).context("creating hard link")?;
                    }
                    TransferKind::Move => {
                        fs::rename(file, to).context("moving file")?;
                    }
                    TransferKind::Copy => {
                        fs::copy(file, to).context("copying file")?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the extension of the source file.
    pub(crate) fn ext<'a>(&'a self, source: &'a Source) -> Result<Option<&'a str>> {
        match source {
            Source::File { file } => Ok(self.file(*file)?.extension().and_then(|s| s.to_str())),
            Source::Archive { path, .. } => Ok(path.extension()),
        }
    }

    /// Dump source information.
    pub(crate) fn dump(&self, o: &mut Out<'_>, source: &Source) -> Result<()> {
        match source {
            Source::File { file } => {
                let file = self.file(*file)?;
                o.link("from", file)?;
            }
            Source::Archive { archive, path } => {
                let archive = self.archive(*archive)?;
                o.link(archive.kind, &archive.path)?;
                let mut o = o.indent(1);
                blank!(o, "/{path}");
            }
        }

        Ok(())
    }

    /// Get the file path if the source is a regular file.
    pub(crate) fn as_file<'a>(&'a self, source: &'a Source) -> Result<Option<&'a Path>> {
        match source {
            Source::File { file } => Ok(Some(self.file(*file)?)),
            Source::Archive { .. } => Ok(None),
        }
    }
}

/// A source file for conversion or transfer.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    /// A regular file in the filesystem.
    File {
        /// The path to the file.
        file: FileId,
    },
    /// A file inside an archive.
    Archive {
        /// Archive identifier.
        archive: ArchiveId,
        /// Path inside the archive.
        path: RelativePathBuf,
    },
}
