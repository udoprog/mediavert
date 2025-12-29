use std::fs::File;
use std::io::Read;
use std::path::Path;

use relative_path::RelativePath;
use zip::ZipArchive;
use zip::read::ZipFile;

use crate::error::{Error, Kind};
use crate::{ArchiveMetadata, Entry};

type Result<T> = core::result::Result<T, Error>;

pub(super) fn enumerate(
    archive_path: &Path,
    sources: &mut dyn FnMut(&RelativePath, ArchiveMetadata) -> Result<()>,
) -> Result<()> {
    let reader = File::open(archive_path).map_err(Kind::Open)?;
    let mut archive = ZipArchive::new(reader).map_err(Kind::ZipOpen)?;

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| Kind::ZipByIndex(e, i))?;

        let m = ArchiveMetadata { size: file.size() };

        sources(RelativePath::new(file.name()), m)?;
    }

    Ok(())
}

pub(super) fn contents(archive_path: &Path, path: &RelativePath) -> Result<Option<Vec<u8>>> {
    let reader = File::open(archive_path).map_err(Kind::Open)?;
    let mut archive = ZipArchive::new(reader).map_err(Kind::ZipOpen)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| Kind::ZipByIndex(e, i))?;

        if file.name() != path.as_str() {
            continue;
        }

        let mut contents = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut contents)
            .map_err(Kind::ReadContents)?;
        return Ok(Some(contents));
    }

    Ok(None)
}

pub(super) fn read(
    archive_path: &Path,
    reader: &mut dyn FnMut(&mut dyn Entry) -> Result<()>,
) -> Result<()> {
    struct ZipEntry<'a> {
        file: ZipFile<'a, File>,
    }

    impl Entry for ZipEntry<'_> {
        fn path(&self) -> Option<&RelativePath> {
            Some(RelativePath::new(self.file.name()))
        }

        fn read_to_end(&mut self, out: &mut Vec<u8>) -> Result<()> {
            out.reserve(self.file.size() as usize);
            self.file.read_to_end(out).map_err(Kind::ReadContents)?;
            Ok(())
        }
    }

    let file = File::open(archive_path).map_err(Kind::Open)?;
    let mut archive = ZipArchive::new(file).map_err(Kind::ZipOpen)?;

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| Kind::ZipByIndex(e, i))?;
        let mut entry = ZipEntry { file };
        reader(&mut entry)?;
    }

    Ok(())
}
