mod _7z;
mod rar;
mod zip;

use core::fmt;
use core::str::FromStr;

use std::path::Path;

use anyhow::Result;
use relative_path::RelativePath;

pub(crate) struct ArchiveErr;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Archive {
    Zip,
    Rar,
    _7z,
}

impl Archive {
    #[inline]
    pub(crate) fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "zip" => Some(Archive::Zip),
            "rar" => Some(Archive::Rar),
            "7z" => Some(Archive::_7z),
            _ => None,
        }
    }
}

impl Archive {
    /// Enumerate an archive of the current type.
    pub(crate) fn enumerate(
        &self,
        path: &Path,
        sources: &mut dyn FnMut(&RelativePath) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::Rar => self::rar::enumerate(path, sources),
            Self::Zip => self::zip::enumerate(path, sources),
            Self::_7z => self::_7z::enumerate(path, sources),
        }
    }

    /// Extract the contents of a file inside the archive.
    pub(crate) fn contents(
        &self,
        archive_path: &Path,
        path: &RelativePath,
    ) -> Result<Option<Vec<u8>>> {
        match self {
            Archive::Rar => self::rar::contents(archive_path, path),
            Archive::Zip => self::zip::contents(archive_path, path),
            Archive::_7z => self::_7z::contents(archive_path, path),
        }
    }
}

impl fmt::Display for Archive {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Archive::Zip => write!(f, "zip"),
            Archive::Rar => write!(f, "rar"),
            Archive::_7z => write!(f, "7z"),
        }
    }
}

impl FromStr for Archive {
    type Err = ArchiveErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "zip" => Ok(Archive::Zip),
            "rar" => Ok(Archive::Rar),
            "7z" => Ok(Archive::_7z),
            _ => Err(ArchiveErr),
        }
    }
}
