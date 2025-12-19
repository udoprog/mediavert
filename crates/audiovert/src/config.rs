use core::fmt;

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::archive::Archive;
use crate::bitrates::Bitrates;
use crate::condition::Condition;
use crate::format::Format;
use crate::meta;
use crate::out::{Out, blank, error, info};
use crate::shell;
use crate::tasks::{MatchingConversion, PathError, Task, TaskKind, Tasks, TransferKind};

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

        for path in &self.paths {
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
                    if let Some(archive) = Archive::from_ext(ext) {
                        dbg!(archive);
                        continue;
                    };

                    tasks
                        .unsupported_extensions
                        .push((from_path.to_path_buf(), ext.to_string()));
                    continue;
                };

                to_formats.clear();

                for conversion in &self.conversion {
                    to_formats.extend(conversion.to_format(from));
                }

                if !to_formats.is_empty() && self.verbose {
                    tasks.matching_conversions.push(MatchingConversion {
                        from_path: from_path.to_path_buf(),
                        from: from,
                        to_formats: to_formats.iter().cloned().collect(),
                    });
                }

                let id_parts = if self.meta {
                    let id_parts = meta::Parts::from_path(
                        from_path,
                        &mut tag_errors,
                        (self.meta_dump || self.meta_dump_error).then_some(&mut tag_items),
                    );

                    let has_errors = !tag_errors.is_empty();

                    if !tag_errors.is_empty() {
                        tasks.errors.push(PathError {
                            path: from_path.to_path_buf(),
                            messages: tag_errors.drain(..).collect(),
                        });
                    }

                    if !tag_items.is_empty() {
                        if self.meta_dump || (self.meta_dump_error && has_errors) {
                            tasks.meta_dumps.push(meta::Dump {
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
                                let Some(suffix) = from_path.strip_prefix(path).ok() else {
                                    tasks.errors.push(PathError {
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
                        if !self.force {
                            tasks
                                .already_exists
                                .push((from_path.to_path_buf(), to_path.clone()));
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
                            kind: if self.r#move {
                                TransferKind::Move
                            } else {
                                TransferKind::Link
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
                        from_path: from_path.to_path_buf(),
                        to_path,
                        moved: exists,
                        pre_remove,
                    });
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
