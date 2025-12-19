use std::fs::File;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use relative_path::RelativePath;
use sevenz_rust2::{Archive, BlockDecoder, Password};

pub(super) fn enumerate(
    archive_path: &Path,
    sources: &mut dyn FnMut(&RelativePath) -> Result<()>,
) -> Result<()> {
    let mut file = File::open(archive_path)?;
    let password = sevenz_rust2::Password::empty();

    let archive = Archive::read(&mut file, &password).context("opening archive")?;

    let block_count = archive.blocks.len();

    for block_index in 0..block_count {
        let dec = BlockDecoder::new(1, block_index, &archive, &password, &mut file);

        for entry in dec.entries() {
            sources(RelativePath::new(entry.name()))?;
        }
    }

    Ok(())
}

pub(super) fn contents(archive_path: &Path, path: &RelativePath) -> Result<Option<Vec<u8>>> {
    let mut file = File::open(archive_path)?;
    let password = Password::empty();

    let archive = Archive::read(&mut file, &password).context("opening archive")?;

    let block_count = archive.blocks.len();

    for block_index in 0..block_count {
        let dec = BlockDecoder::new(1, block_index, &archive, &password, &mut file);

        let found = dec.entries().iter().any(|e| e.name() == path);

        if !found {
            continue;
        }

        let mut contents = Vec::new();

        dec.for_each_entries(&mut |entry, reader| {
            if entry.name() == path {
                io::copy(reader, &mut contents)?;
                Ok(false)
            } else {
                io::copy(reader, &mut io::sink())?;
                Ok(true)
            }
        })?;

        return Ok(Some(contents));
    }

    Ok(None)
}
