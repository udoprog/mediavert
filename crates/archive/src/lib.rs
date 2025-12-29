mod _7z;
mod error;
mod rar;
mod zip;

use core::fmt;
use core::str::FromStr;

use std::path::Path;

use relative_path::RelativePath;

pub use self::error::{ArchiveErr, Error};

type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug, Clone, Copy)]
pub enum Archive {
    Zip,
    Rar,
    _7z,
}

impl Archive {
    #[inline]
    pub fn from_ext(ext: &str) -> Option<Self> {
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
    pub fn enumerate(
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
    pub fn contents(&self, archive_path: &Path, path: &RelativePath) -> Result<Option<Vec<u8>>> {
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
    fn from_str(s: &str) -> Result<Self, ArchiveErr> {
        match s {
            "zip" => Ok(Archive::Zip),
            "rar" => Ok(Archive::Rar),
            "7z" => Ok(Archive::_7z),
            _ => Err(ArchiveErr),
        }
    }
}
