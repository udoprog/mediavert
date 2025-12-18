use std::collections::BTreeSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::rc::Rc;

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

/// Data about a page.
pub struct Page {
    /// The filesystem name of the page.
    pub path: PathBuf,
    /// The name of the page.
    pub name: String,
    /// The filesystem metadata of the page.
    pub metadata: Metadata,
}

/// Data about a book.
pub struct Book {
    /// The directory where the book is located.
    pub dir: PathBuf,
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
        (&self.name, &self.dir)
    }

    /// Returns the total size of all pages in bytes.
    #[inline]
    pub fn bytes(&self) -> u64 {
        self.pages.iter().map(|page| page.metadata.len()).sum()
    }
}
