use core::fmt;

use std::ffi::OsString;
use std::path::PathBuf;

use crate::config::{Archives, Source};
use crate::format::Format;
use crate::meta::Dump;

pub(crate) struct Tasks {
    pub(crate) meta_dumps: Vec<Dump>,
    pub(crate) errors: Vec<PathError>,
    pub(crate) matching_conversions: Vec<MatchingConversion>,
    pub(crate) tasks: Vec<Task>,
    pub(crate) to_trash: Vec<Trash>,
    pub(crate) already_exists: Vec<Exists>,
    pub(crate) unsupported: Vec<Unsupported>,
    pub(crate) archives: Archives,
}

impl Tasks {
    pub(crate) fn new() -> Self {
        Self {
            meta_dumps: Vec::new(),
            errors: Vec::new(),
            matching_conversions: Vec::new(),
            tasks: Vec::new(),
            to_trash: Vec::new(),
            already_exists: Vec::new(),
            unsupported: Vec::new(),
            archives: Archives::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TransferKind {
    Copy,
    Link,
    Move,
}

impl TransferKind {
    #[inline]
    pub(crate) fn symbolic_command(&self) -> &'static str {
        match self {
            TransferKind::Copy => "cp",
            TransferKind::Link => "ln",
            TransferKind::Move => "mv",
        }
    }
}

impl fmt::Display for TransferKind {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransferKind::Copy => write!(f, "copying"),
            TransferKind::Link => write!(f, "link"),
            TransferKind::Move => write!(f, "move"),
        }
    }
}

/// The kind of a task.
pub(crate) enum TaskKind {
    /// Convert from one format to another.
    Convert {
        part_path: PathBuf,
        from: Format,
        to: Format,
        converted: bool,
    },
    /// Transfer from source to destination.
    Transfer {
        /// The kind of the transfer.
        kind: TransferKind,
    },
}

impl TaskKind {
    #[inline]
    pub(crate) fn is_completed(&self) -> bool {
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

/// A collection of errors associated with a particular path.
pub(crate) struct PathError {
    pub(crate) source: Source,
    pub(crate) messages: Vec<String>,
}

/// A prepared task for conversion or transfer.
pub(crate) struct Task {
    pub(crate) index: usize,
    pub(crate) kind: TaskKind,
    pub(crate) source: Source,
    pub(crate) to_path: PathBuf,
    pub(crate) moved: bool,
    pub(crate) pre_remove: Vec<(&'static str, PathBuf)>,
}

impl Task {
    pub(crate) fn is_completed(&self) -> bool {
        self.kind.is_completed() && self.moved && self.pre_remove.is_empty()
    }
}

pub(crate) struct MatchingConversion {
    pub(crate) source: Source,
    pub(crate) from: Format,
    pub(crate) to_formats: Vec<Format>,
}

pub(crate) enum TrashWhat {
    SourceFile,
}

impl fmt::Display for TrashWhat {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceFile => write!(f, "source file"),
        }
    }
}

pub(crate) struct Trash {
    pub(crate) what: TrashWhat,
    pub(crate) path: PathBuf,
    pub(crate) name: OsString,
}

pub(crate) struct Exists {
    pub(crate) source: Source,
    pub(crate) path: PathBuf,
    pub(crate) absolute_path: PathBuf,
}

pub(crate) struct Unsupported {
    pub(crate) source: Source,
    pub(crate) ext: String,
}
