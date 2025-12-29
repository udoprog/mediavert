use std::fs::File;
use std::io;
use std::path::Path;

use relative_path::RelativePath;
use sevenz_rust2::{Archive, ArchiveEntry, BlockDecoder, Password};

use crate::error::{Error, Kind};
use crate::{ArchiveMetadata, Entry};

type Result<T> = core::result::Result<T, Error>;

pub(super) fn enumerate(
    archive_path: &Path,
    sources: &mut dyn FnMut(&RelativePath, ArchiveMetadata) -> Result<()>,
) -> Result<()> {
    let mut file = File::open(archive_path).map_err(Kind::Open)?;
    let password = sevenz_rust2::Password::empty();

    let archive = Archive::read(&mut file, &password).map_err(Kind::SevenZipRead)?;

    let block_count = archive.blocks.len();

    for block_index in 0..block_count {
        let dec = BlockDecoder::new(1, block_index, &archive, &password, &mut file);

        for entry in dec.entries() {
            let m = ArchiveMetadata { size: entry.size() };

            sources(RelativePath::new(entry.name()), m)?;
        }
    }

    Ok(())
}

pub(super) fn contents(archive_path: &Path, path: &RelativePath) -> Result<Option<Vec<u8>>> {
    let mut file = File::open(archive_path).map_err(Kind::Open)?;
    let password = Password::empty();

    let archive = Archive::read(&mut file, &password).map_err(Kind::SevenZipRead)?;

    let block_count = archive.blocks.len();

    for block_index in 0..block_count {
        let dec = BlockDecoder::new(1, block_index, &archive, &password, &mut file);

        let found = dec.entries().iter().any(|e| e.name() == path);

        if !found {
            continue;
        }

        let mut contents = Vec::new();

        let result = dec.for_each_entries(&mut |entry, reader| {
            if entry.name() == path {
                io::copy(reader, &mut contents)?;
                Ok(false)
            } else {
                io::copy(reader, &mut io::sink())?;
                Ok(true)
            }
        });

        if let Err(e) = result {
            return Err(Error::new(Kind::SevenZipRead(e)));
        }

        return Ok(Some(contents));
    }

    Ok(None)
}

pub(super) fn read(
    archive_path: &Path,
    r: &mut dyn FnMut(&mut dyn Entry) -> Result<()>,
) -> Result<()> {
    struct SevenZipEntry<'a> {
        entry: &'a ArchiveEntry,
        reader: &'a mut dyn io::Read,
        read: bool,
    }

    impl Entry for SevenZipEntry<'_> {
        fn path(&self) -> Option<&RelativePath> {
            Some(RelativePath::new(self.entry.name()))
        }

        fn read_to_end(&mut self, out: &mut Vec<u8>) -> Result<()> {
            io::copy(self.reader, out).map_err(Kind::ReadContents)?;
            self.read = true;
            Ok(())
        }
    }

    let mut file = File::open(archive_path).map_err(Kind::Open)?;
    let password = Password::empty();

    let archive = Archive::read(&mut file, &password).map_err(Kind::SevenZipRead)?;

    let block_count = archive.blocks.len();
    let mut err = None;

    for block_index in 0..block_count {
        let dec = BlockDecoder::new(1, block_index, &archive, &password, &mut file);

        let result = dec.for_each_entries(&mut |entry, reader| {
            let mut entry = SevenZipEntry {
                entry,
                reader,
                read: false,
            };

            if let Err(error) = r(&mut entry) {
                err = Some(error);
                return Ok(false);
            }

            if !entry.read {
                io::copy(reader, &mut io::sink())?;
            }

            Ok(true)
        });

        if let Err(e) = result {
            return Err(Error::new(Kind::SevenZipRead(e)));
        }

        if let Some(e) = err {
            return Err(e);
        }
    }

    Ok(())
}
