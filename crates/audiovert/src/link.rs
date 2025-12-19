use core::ops::Deref;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// An element that might be linkable.
pub(crate) trait Linkable {
    /// Get the path of the linkable element.
    fn path(&self) -> &Path;

    /// Get the link of the linkable element.
    fn link(&self) -> Option<&Path>;
}

/// A path that is guaranteed to be linkable.
#[derive(Clone)]
pub(crate) struct Link {
    path: PathBuf,
    abs: PathBuf,
}

impl Link {
    #[inline]
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let abs = path.canonicalize()?;
        Ok(Self {
            path: path.to_owned(),
            abs,
        })
    }
}

impl Deref for Link {
    type Target = Path;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl AsRef<Path> for Link {
    #[inline]
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Linkable for Link {
    #[inline]
    fn path(&self) -> &Path {
        &self.path
    }

    #[inline]
    fn link(&self) -> Option<&Path> {
        Some(&self.abs)
    }
}

/// A path that might be linkable.
#[derive(Clone)]
pub(crate) struct MaybeLink {
    path: PathBuf,
    abs: Option<PathBuf>,
}

impl MaybeLink {
    pub(crate) fn new(path: PathBuf) -> Self {
        let abs = path.canonicalize().ok();
        Self { path, abs }
    }
}

impl Deref for MaybeLink {
    type Target = Path;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl AsRef<Path> for MaybeLink {
    #[inline]
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for MaybeLink {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl Linkable for MaybeLink {
    #[inline]
    fn path(&self) -> &Path {
        &self.path
    }

    #[inline]
    fn link(&self) -> Option<&Path> {
        self.abs.as_deref()
    }
}
