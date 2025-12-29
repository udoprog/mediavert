use std::path::Path;

use relative_path::RelativePath;
use unrar::{Archive, CursorBeforeFile, CursorBeforeHeader, OpenArchive, Process};

use crate::error::{Error, Kind};
use crate::{ArchiveMetadata, Entry};

type Result<T, E = Error> = core::result::Result<T, E>;

pub(super) fn enumerate(
    archive_path: &Path,
    sources: &mut dyn FnMut(&RelativePath, ArchiveMetadata) -> Result<()>,
) -> Result<()> {
    let archive = Archive::new(archive_path);
    let open_archive = archive.open_for_listing().map_err(Kind::UnrarOpen)?;

    for e in open_archive {
        let e = e.map_err(Kind::UnrarRead)?;

        let Some(name) = e.filename.as_os_str().to_str() else {
            continue;
        };

        let m = ArchiveMetadata {
            size: e.unpacked_size,
        };

        sources(RelativePath::new(name), m)?;
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

pub(super) fn read(
    archive_path: &Path,
    reader: &mut dyn FnMut(&mut dyn Entry) -> Result<()>,
) -> Result<()> {
    struct RarEntry {
        archive: Option<OpenArchive<Process, CursorBeforeFile>>,
        next: Option<OpenArchive<Process, CursorBeforeHeader>>,
    }

    impl Entry for RarEntry {
        fn path(&self) -> Option<&RelativePath> {
            let path = self.archive.as_ref()?.entry().filename.to_str()?;
            Some(RelativePath::new(path))
        }

        fn read_to_end(&mut self, out: &mut Vec<u8>) -> Result<()> {
            let archive = self.archive.take().ok_or(Kind::MissingArchive)?;
            let (contents, next) = archive.read().map_err(Kind::UnrarReadContents)?;
            self.next = Some(next);
            out.extend_from_slice(&contents);
            Ok(())
        }
    }

    let archive = Archive::new(archive_path);
    let mut archive = archive.open_for_processing().map_err(Kind::UnrarOpen)?;

    while let Some(a) = archive.read_header().map_err(Kind::UnrarReadHeader)? {
        let mut entry = RarEntry {
            archive: Some(a),
            next: None,
        };

        reader(&mut entry)?;

        archive = match entry.next.take() {
            Some(next) => next,
            None => {
                let archive = entry.archive.take().ok_or(Kind::MissingArchive)?;
                archive.skip().map_err(Kind::UnrarSkip)?
            }
        };
    }

    Ok(())
}
