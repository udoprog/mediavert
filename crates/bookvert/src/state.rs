use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result};
use archive::Archive;
use relative_path::{RelativePath, RelativePathBuf};

/// The state of a bookvert session.
#[derive(Default)]
pub struct State {
    /// The name of the series.
    pub name: Option<String>,
    /// The filesystem detected name of the series. These can be used to support
    /// an interactive session where you can for example pick names from a list.
    pub names: BTreeSet<String>,
    /// The detected catalogs in the session.
    pub catalogs: Vec<Catalog>,
}

impl State {
    /// Count the number of catalogs which have a picked book.
    #[inline]
    pub(crate) fn picked(&self) -> usize {
        self.catalogs.iter().filter(|c| c.picked.is_some()).count()
    }
}

/// The state for a single catalog.
pub struct Catalog {
    /// The catalog number.
    pub number: u32,
    /// The books in the catalog.
    pub books: Vec<Rc<Book>>,
    /// The picked book.
    pub picked: Option<usize>,
}

impl Catalog {
    /// Returns the selected book, if any.
    #[inline]
    pub fn selected(&self) -> Option<&Book> {
        Some(self.books.get(self.picked?)?.as_ref())
    }
}

/// Metadata about a page.
pub struct PageMetadata {
    /// The size in bytes of a page.
    pub size: u64,
}

/// Data about a page.
pub struct Page {
    /// The filesystem name of the page.
    pub path: RelativePathBuf,
    /// The name of the page.
    pub name: String,
    /// The filesystem metadata of the page.
    pub metadata: PageMetadata,
}

/// The source of a book.
pub struct BookSource {
    pub path: PathBuf,
    pub kind: BookSourceType,
}

/// The type of source for a book.
pub enum BookSourceType {
    Directory,
    Archive(Archive),
}

/// Data about a book.
pub struct Book {
    /// The directory where the book is located.
    pub source: BookSource,
    /// The name of the book.
    pub name: String,
    /// The pages in the book.
    pub pages: Vec<Page>,
    /// The series numbers associated with the book.
    pub numbers: BTreeSet<u32>,
}

impl Book {
    /// Returns a key for sorting books by name and directory.
    #[inline]
    pub fn key(&self) -> (&str, &Path) {
        (&self.name, &self.source.path)
    }

    /// Returns the total size of all pages in bytes.
    #[inline]
    pub fn bytes(&self) -> u64 {
        self.pages.iter().map(|page| page.metadata.size).sum()
    }

    /// Get the raw contents of a page.
    pub fn contents(&self, path: &RelativePath) -> Result<Vec<u8>> {
        match &self.source.kind {
            BookSourceType::Directory => {
                let path = path.to_path(&self.source.path);

                match fs::read(&path) {
                    Ok(data) => Ok(data),
                    Err(e) => Err(e).context(path.display().to_string()),
                }
            }
            BookSourceType::Archive(archive) => match archive.contents(&self.source.path, path) {
                Ok(Some(data)) => Ok(data),
                Ok(None) => Err(anyhow::anyhow!(
                    "file not found in archive: {}",
                    self.source.path.display()
                )),
                Err(e) => Err(e).context(self.source.path.display().to_string()),
            },
        }
    }
}
