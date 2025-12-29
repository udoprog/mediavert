use std::fs::File;
use std::io::Read;
use std::path::Path;

use relative_path::RelativePath;
use zip::ZipArchive;

use crate::ArchiveMetadata;
use crate::error::{Error, Kind};

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
