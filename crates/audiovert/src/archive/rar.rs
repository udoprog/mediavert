use std::path::Path;

use anyhow::Result;
use relative_path::RelativePath;
use unrar::Archive;

pub(super) fn enumerate(
    archive_path: &Path,
    sources: &mut dyn FnMut(&RelativePath) -> Result<()>,
) -> Result<()> {
    let archive = Archive::new(archive_path);
    let open_archive = archive.open_for_listing()?;

    for e in open_archive {
        let e = e?;

        let Some(name) = e.filename.as_os_str().to_str() else {
            continue;
        };

        sources(RelativePath::new(name))?;
    }

    Ok(())
}

pub(super) fn contents(archive_path: &Path, path: &RelativePath) -> Result<Option<Vec<u8>>> {
    let archive = Archive::new(archive_path);
    let mut archive = archive.open_for_processing()?;

    while let Some(a) = archive.read_header()? {
        if a.entry().filename.to_str() == Some(path.as_str()) {
            let (contents, _) = a.read()?;
            return Ok(Some(contents));
        }

        archive = a.skip()?;
    }

    Ok(None)
}
