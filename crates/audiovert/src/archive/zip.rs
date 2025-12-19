use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use relative_path::RelativePath;
use zip::ZipArchive;

pub(super) fn enumerate(
    archive_path: &Path,
    sources: &mut dyn FnMut(&RelativePath) -> Result<()>,
) -> Result<()> {
    let reader = File::open(archive_path)?;
    let mut archive = ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        sources(RelativePath::new(file.name()))?;
    }

    Ok(())
}

pub(super) fn contents(
    archive_path: &Path,
    path: &RelativePath,
) -> anyhow::Result<Option<Vec<u8>>> {
    let reader = File::open(archive_path)?;
    let mut archive = ZipArchive::new(reader).context("opening archive")?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        if file.name() != path.as_str() {
            continue;
        }

        let mut contents = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut contents)?;
        return Ok(Some(contents));
    }

    Ok(None)
}
