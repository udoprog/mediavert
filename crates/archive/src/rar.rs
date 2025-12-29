use std::path::Path;

use relative_path::RelativePath;
use unrar::Archive;

use crate::error::{Error, Kind};

type Result<T> = core::result::Result<T, Error>;

pub(super) fn enumerate(
    archive_path: &Path,
    sources: &mut dyn FnMut(&RelativePath) -> Result<()>,
) -> Result<()> {
    let archive = Archive::new(archive_path);
    let open_archive = archive.open_for_listing().map_err(Kind::UnrarOpen)?;

    for e in open_archive {
        let e = e.map_err(Kind::UnrarRead)?;

        let Some(name) = e.filename.as_os_str().to_str() else {
            continue;
        };

        sources(RelativePath::new(name))?;
    }

    Ok(())
}

pub(super) fn contents(archive_path: &Path, path: &RelativePath) -> Result<Option<Vec<u8>>> {
    let archive = Archive::new(archive_path);
    let mut archive = archive.open_for_processing().map_err(Kind::UnrarOpen)?;

    while let Some(a) = archive.read_header().map_err(Kind::UnrarReadHeader)? {
        if a.entry().filename.to_str() == Some(path.as_str()) {
            let (contents, _) = a.read().map_err(Kind::UnrarReadContents)?;
            return Ok(Some(contents));
        }

        archive = a.skip().map_err(Kind::UnrarSkip)?;
    }

    Ok(None)
}
