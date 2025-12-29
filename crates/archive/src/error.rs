use core::fmt;

use std::io;

#[derive(Debug)]
#[non_exhaustive]
pub struct ArchiveErr;

impl fmt::Display for ArchiveErr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported archive format")
    }
}

impl core::error::Error for ArchiveErr {}

pub struct Error {
    kind: Kind,
}

impl Error {
    #[inline]
    pub(super) fn new(kind: Kind) -> Self {
        Self { kind }
    }
}

impl core::error::Error for Error {
    #[inline]
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match &self.kind {
            Kind::Open(e) => Some(e),
            Kind::ReadContents(e) => Some(e),
            Kind::ZipOpen(e) => Some(e),
            Kind::ZipByIndex(e, _) => Some(e),
            Kind::SevenZipRead(e) => Some(e),
            Kind::UnrarOpen(e) => Some(e),
            Kind::UnrarRead(e) => Some(e),
            Kind::UnrarReadHeader(e) => Some(e),
            Kind::UnrarReadContents(e) => Some(e),
            Kind::UnrarSkip(e) => Some(e),
        }
    }
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl fmt::Debug for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl From<Kind> for Error {
    #[inline]
    fn from(kind: Kind) -> Self {
        Self::new(kind)
    }
}

#[derive(Debug)]
pub(super) enum Kind {
    Open(io::Error),
    ReadContents(io::Error),
    ZipOpen(zip::result::ZipError),
    ZipByIndex(zip::result::ZipError, usize),
    SevenZipRead(sevenz_rust2::Error),
    UnrarOpen(unrar::error::UnrarError),
    UnrarRead(unrar::error::UnrarError),
    UnrarReadHeader(unrar::error::UnrarError),
    UnrarReadContents(unrar::error::UnrarError),
    UnrarSkip(unrar::error::UnrarError),
}

impl fmt::Display for Kind {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Kind::Open(..) => write!(f, "failed to open archive"),
            Kind::ReadContents(..) => write!(f, "failed to read contents from archive"),
            Kind::ZipOpen(..) => write!(f, "failed to read zip archive"),
            Kind::ZipByIndex(_, index) => {
                write!(f, "failed to access file at index {index} in zip archive")
            }
            Kind::SevenZipRead(..) => write!(f, "failed to read 7z archive"),
            Kind::UnrarOpen(..) => write!(f, "failed to open rar archive"),
            Kind::UnrarRead(..) => write!(f, "failed to read from rar archive"),
            Kind::UnrarReadHeader(..) => write!(f, "failed to read rar archive header"),
            Kind::UnrarReadContents(..) => write!(f, "failed to read rar archive contents"),
            Kind::UnrarSkip(..) => write!(f, "failed to skip rar archive entry"),
        }
    }
}
