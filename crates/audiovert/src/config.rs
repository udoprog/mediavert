use core::fmt;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::bitrates::Bitrates;
use crate::condition::Condition;
use crate::format::Format;
use crate::out::{Out, blank, error, info};
use crate::shell;

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
