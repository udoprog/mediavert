use std::io::Read;

use relative_path::RelativePath;

use crate::error::{Error, Kind};

/// An entry inside an archive.
pub trait Entry {
    /// The path of the entry inside the archive.
    fn path(&self) -> Option<&RelativePath>;

    /// Read the contents of the entry.
    fn read_to_end(&mut self, out: &mut Vec<u8>) -> Result<(), Error>;
}

/// An entry based on a custom reader.
pub struct ReaderEntry<'a, R> {
    path: &'a RelativePath,
    file: R,
}

impl<'a, R> ReaderEntry<'a, R> {
    /// Create a new reader entry.
    pub fn new(path: &'a RelativePath, file: R) -> Self {
        Self { path, file }
    }
}

impl<R> Entry for ReaderEntry<'_, R>
where
    R: Read,
{
    fn path(&self) -> Option<&RelativePath> {
        Some(self.path)
    }

    fn read_to_end(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        self.file.read_to_end(out).map_err(Kind::ReadContents)?;
        Ok(())
    }
}
