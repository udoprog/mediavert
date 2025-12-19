use core::fmt;
use core::str::FromStr;

pub(crate) struct ArchiveErr;

pub(crate) enum Archive {
    Zip,
    Rar,
}

impl Archive {
    #[inline]
    pub(crate) fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "zip" => Some(Archive::Zip),
            "rar" => Some(Archive::Rar),
            _ => None,
        }
    }
}

impl fmt::Display for Archive {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Archive::Zip => write!(f, "zip"),
            Archive::Rar => write!(f, "rar"),
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
            _ => Err(ArchiveErr),
        }
    }
}
