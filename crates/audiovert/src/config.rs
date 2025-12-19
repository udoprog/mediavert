use core::fmt;

use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::archive::Archive;
use crate::bitrates::Bitrates;
use crate::condition::Condition;
use crate::format::Format;
use crate::meta;
use crate::out::{Out, blank, error, info};
use crate::shell;
use crate::tasks::{
    MatchingConversion, PathError, Task, TaskKind, Tasks, TransferKind, Unsupported,
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

        for path in &self.paths {
            for f in ignore::Walk::new(path) {
                let entry = f?;

                let walked = entry.path();

                if !walked.is_file() {
                    continue;
                }

                let Some(ext) = walked.extension().and_then(|s| s.to_str()) else {
                    continue;
                };

                if let Some(kind) = Archive::from_ext(ext) {
                    let archive_id = tasks.archives.push(ArchiveOrigin {
                        kind,
                        path: walked.to_path_buf(),
                    });

                    let origin = Origin::Archive(archive_id);

                    match kind {
                        Archive::Rar => {
                            let archive = unrar::Archive::new(walked);
                            let open_archive = archive.open_for_listing()?;

                            for e in open_archive {
                                let e = e?;

                                if e.filename.is_absolute() {
                                    continue;
                                }

                                sources.push(Source {
                                    origin,
                                    path: e.filename.clone(),
                                });
                            }
                        }
                        Archive::Zip => {
                            let reader = File::open(walked)?;
                            let mut archive = zip::ZipArchive::new(reader)?;

                            for i in 0..archive.len() {
                                let file = archive.by_index(i)?;

                                let Some(path) = file.enclosed_name() else {
                                    continue;
                                };

                                if path.is_absolute() {
                                    continue;
                                }

                                sources.push(Source { origin, path });
                            }
                        }
                    }
                } else {
                    let source = Source {
                        origin: Origin::File,
                        path: walked.to_path_buf(),
                    };

                    sources.push(source);
                }

                for source in sources.drain(..) {
                    let Some(from) = source.ext().and_then(Format::from_ext) else {
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

                    let id_parts = if self.meta {
                        let id_parts = meta::Parts::from_path(
                            &source,
                            &tasks.archives,
                            &mut tag_errors,
                            (self.meta_dump || self.meta_dump_error).then_some(&mut tag_items),
                        );

                        let id_parts = match id_parts {
                            Ok(id_parts) => Some(id_parts),
                            Err(e) => {
                                tag_errors.push(format!("failed to read tags: {e}"));
                                None
                            }
                        };

                        let has_errors = !tag_errors.is_empty();

                        if !tag_errors.is_empty() {
                            tasks.errors.push(PathError {
                                source: source.clone(),
                                messages: tag_errors.drain(..).collect(),
                            });
                        }

                        if !tag_items.is_empty() {
                            if self.meta_dump || (self.meta_dump_error && has_errors) {
                                tasks.meta_dumps.push(meta::Dump {
                                    source: source.clone(),
                                    items: tag_items.drain(..).collect(),
                                });
                            }

                            tag_items.clear();
                        }

                        id_parts
                    } else {
                        None
                    };

                    for &to in &to_formats {
                        let mut pre_remove = Vec::new();

                        let to_path = if let Some(to_dir) = &self.to_dir {
                            match &id_parts {
                                Some(id_parts) => {
                                    let mut to_path = to_dir.to_path_buf();
                                    id_parts.append_to(&mut to_path);
                                    to_path.add_extension(to.ext());
                                    to_path
                                }
                                None => {
                                    let mut to_path = to_dir.clone();

                                    if let Err(e) =
                                        source.to_path(path, &tasks.archives, &mut to_path)
                                    {
                                        tasks.errors.push(PathError {
                                            source: source.clone(),
                                            messages: vec![format!(
                                                "failed to get source path: {e}"
                                            )],
                                        });

                                        continue;
                                    };

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
                                    let mut to_path = source.path.to_path_buf();
                                    to_path.set_extension(to.ext());
                                    to_path
                                }
                            }
                        };

                        if source.path == to_path {
                            continue;
                        }

                        let exists = if to_path.exists() {
                            if !self.force {
                                tasks.already_exists.push((source.clone(), to_path.clone()));
                                true
                            } else {
                                pre_remove.push(("destination path (--force)", to_path.clone()));
                                false
                            }
                        } else {
                            false
                        };

                        let kind = if from == to && !self.forced_bitrates.contains(&from) {
                            TaskKind::Transfer {
                                kind: if source.origin.is_file() {
                                    if self.r#move {
                                        TransferKind::Move
                                    } else {
                                        TransferKind::Link
                                    }
                                } else {
                                    TransferKind::Copy
                                },
                            }
                        } else {
                            let part_path = to_path.with_added_extension(&self.part_ext);

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
                            pre_remove,
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

/// Origin of a file inside an archive.
#[derive(Clone)]
pub(crate) struct ArchiveOrigin {
    /// Kind of the archive.
    pub(crate) kind: Archive,
    /// Path to the archive.
    pub(crate) path: PathBuf,
}

impl ArchiveOrigin {
    /// Get the contents of a file inside the archive.
    pub(crate) fn contents(&self, path: &Path) -> Result<Vec<u8>> {
        match self.kind {
            Archive::Rar => {
                let archive = unrar::Archive::new(&self.path);
                let mut archive = archive.open_for_processing()?;

                while let Some(a) = archive.read_header()? {
                    if a.entry().filename == path {
                        let (contents, _) = a.read()?;
                        return Ok(contents);
                    }

                    archive = a.skip()?;
                }
            }
            Archive::Zip => {
                let reader = File::open(&self.path)?;
                let mut archive = zip::ZipArchive::new(reader)?;

                for i in 0..archive.len() {
                    let mut file = archive.by_index(i)?;

                    let Some(file_path) = file.enclosed_name() else {
                        continue;
                    };

                    if file_path != path {
                        continue;
                    }

                    let mut contents = Vec::with_capacity(file.size() as usize);
                    file.read_to_end(&mut contents)?;
                    return Ok(contents);
                }
            }
        }

        Err(anyhow!(
            "file {} not found in archive {:?}",
            path.display(),
            self.path
        ))
    }
}

/// Origin of a source file.
#[derive(Clone, Copy)]
pub(crate) enum Origin {
    /// A regular file in the filesystem.
    File,
    /// A file inside an archive.
    Archive(ArchiveId),
}

impl Origin {
    /// Check if the origin is a regular file.
    #[inline]
    pub(crate) fn is_file(&self) -> bool {
        matches!(self, Origin::File)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ArchiveId(usize);

impl fmt::Display for ArchiveId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Collection of archives.
pub(crate) struct Archives {
    archives: Vec<ArchiveOrigin>,
}

impl Archives {
    /// Construct a new collection of archives.
    pub(crate) fn new() -> Self {
        Archives {
            archives: Vec::new(),
        }
    }

    pub(crate) fn get(&self, id: ArchiveId) -> Result<&ArchiveOrigin> {
        let Some(archive) = self.archives.get(id.0) else {
            return Err(anyhow!("invalid archive id: {id}"));
        };

        Ok(archive)
    }

    /// Push an archive to the collection.
    #[inline]
    pub(crate) fn push(&mut self, archive: ArchiveOrigin) -> ArchiveId {
        let id = ArchiveId(self.archives.len());
        self.archives.push(archive);
        id
    }

    /// Get the contents of the source file.
    pub(crate) fn contents(&self, source: &Source) -> Result<Vec<u8>> {
        match source.origin {
            Origin::File => Ok(fs::read(&source.path)?),
            Origin::Archive(archive) => {
                let Some(archive) = self.archives.get(archive.0) else {
                    anyhow::bail!("invalid archive id: {archive}");
                };

                archive.contents(&source.path)
            }
        }
    }
}

/// A source file for conversion or transfer.
#[derive(Clone)]
pub(crate) struct Source {
    pub(crate) origin: Origin,
    pub(crate) path: PathBuf,
}

impl Source {
    pub(crate) fn to_path(
        &self,
        path: &Path,
        archives: &Archives,
        out: &mut PathBuf,
    ) -> Result<()> {
        match &self.origin {
            Origin::Archive(archive) => {
                let archive = archives.get(*archive)?;

                let Some(suffix) = archive.path.strip_prefix(path).ok() else {
                    bail!("failed to get suffix for path");
                };

                if let Some(parent) = suffix.parent() {
                    out.push(parent);
                }

                if let Some(file_name) = archive.path.file_stem() {
                    out.push(file_name);
                }

                out.push(&self.path);
                Ok(())
            }
            Origin::File => {
                let Some(suffix) = self.path.strip_prefix(path).ok() else {
                    bail!("failed to get suffix for path");
                };

                out.push(suffix);
                Ok(())
            }
        }
    }

    pub(crate) fn move_to(&self, archives: &Archives, to: &Path, kind: TransferKind) -> Result<()> {
        match &self.origin {
            Origin::Archive(..) => match kind {
                TransferKind::Link => bail!("cannot link from archive"),
                TransferKind::Move => bail!("cannot move from archive"),
                TransferKind::Copy => {
                    let contents = archives.contents(self)?;
                    fs::write(to, contents).context("writing file")?;
                }
            },
            Origin::File => match kind {
                TransferKind::Link => {
                    fs::hard_link(&self.path, to).context("creating hard link")?;
                }
                TransferKind::Move => {
                    fs::rename(&self.path, to).context("moving file")?;
                }
                TransferKind::Copy => {
                    fs::copy(&self.path, to).context("copying file")?;
                }
            },
        }

        Ok(())
    }

    /// Get the extension of the source file.
    pub(crate) fn ext(&self) -> Option<&str> {
        self.path.extension().and_then(|s| s.to_str())
    }

    /// Dump source information.
    pub(crate) fn dump(&self, o: &mut Out<'_>, archives: &Archives) -> Result<()> {
        if let Origin::Archive(archive) = self.origin {
            let archive = archives.get(archive)?;

            blank!(
                o,
                "{}: {}",
                archive.kind,
                shell::escape(archive.path.as_os_str()),
            );
        }

        blank!(o, "path: {}", shell::escape(self.path.as_os_str()));
        Ok(())
    }
}
