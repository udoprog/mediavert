use std::borrow::Cow;
use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context, Result};
use jiff::civil::Date;
use lofty::file::{FileType, TaggedFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, ItemValue, TagItem};

use crate::config::{Archives, Source};
use crate::out::{Out, blank, info};

pub(crate) struct Parts {
    year: i16,
    artist: String,
    album: String,
    track: u32,
    title: String,
    media_type: Option<String>,
    set: Option<(u32, u32)>,
}

impl Parts {
    pub(crate) fn from_path(
        source: &Source,
        archives: &Archives,
        errors: &mut Vec<String>,
        tag_items: Option<&mut Vec<TagItem>>,
    ) -> Result<Self> {
        let file: TaggedFile = if source.origin.is_file() {
            lofty::read_from_path(&source.path)?
        } else {
            let contents = archives.contents(source)?;
            let mut probe = Probe::new(Cursor::new(contents));

            if let Some(file_type) = source.ext().and_then(FileType::from_ext) {
                probe = probe.set_file_type(file_type);
            }

            probe.read()?
        };

        let tag = file.primary_tag().context("missing primary tag")?;

        if let Some(tag_items) = tag_items {
            for item in tag.items() {
                tag_items.push(item.clone());
            }
        }

        macro_rules! get_str {
            ($id:ident) => {{
                let tag = tag.get(&ItemKey::$id);
                tag.and_then(|item| item.value().text())
            }};
        }

        macro_rules! get {
            ($id:ident) => {{ get_str!($id).and_then(|s| s.parse().ok()) }};
        }

        let year: Option<i16> = 'year: {
            fn parse_year(s: &str) -> Option<i16> {
                let s = s.trim();

                if let Ok(date) = s.parse::<Date>() {
                    return Some(date.year());
                }

                if let Ok(year) = s.parse::<i16>() {
                    return Some(year);
                }

                None
            }

            if let Some(d) = get_str!(OriginalReleaseDate).and_then(parse_year) {
                break 'year Some(d);
            };

            if let Some(d) = get_str!(ReleaseDate).and_then(parse_year) {
                break 'year Some(d);
            };

            if let Some(d) = get_str!(Year).and_then(parse_year) {
                break 'year Some(d);
            };

            if let Some(d) = get_str!(RecordingDate).and_then(parse_year) {
                break 'year Some(d);
            };

            errors.push("missing year".to_string());
            None
        };

        let album = 'album: {
            if let Some(album) = get_str!(AlbumTitle) {
                break 'album Some(album.trim());
            };

            errors.push("missing album".to_string());
            None
        };

        let artist = 'artist: {
            if let Some(artist) = get_str!(AlbumArtist) {
                break 'artist Some(artist.trim());
            };

            if let Some(artist) = get_str!(TrackArtist) {
                break 'artist Some(artist.trim());
            };

            errors.push("missing artist".to_string());
            None
        };

        let title = 'title: {
            if let Some(title) = get_str!(TrackTitle) {
                break 'title Some(title.trim());
            };

            errors.push("missing title".to_string());
            None
        };

        let track = 'track: {
            if let Some(track) = get!(TrackNumber) {
                break 'track Some(track);
            };

            errors.push("missing track".to_string());
            None
        };

        let media_type = 'media_type: {
            if let Some(value) = get_str!(OriginalMediaType) {
                break 'media_type Some(value.trim());
            };

            None
        };

        let set = 'set: {
            let Some(number) = get!(DiscNumber) else {
                break 'set None;
            };

            let Some(total) = get!(DiscTotal) else {
                break 'set None;
            };

            Some((number, total))
        };

        let value = || {
            Some(Self {
                year: year?,
                artist: artist?.trim().to_owned(),
                album: album?.to_owned(),
                track: track?,
                title: title?.trim().to_owned(),
                media_type: media_type.map(str::to_owned),
                set,
            })
        };

        value().context("incomplete tag information")
    }

    /// Append parts to a buffer.
    pub(crate) fn append_to(&self, path: &mut PathBuf) {
        use core::fmt::Write;

        let mut s = String::new();

        macro_rules! s {
            ($($arg:tt)*) => {{
                s.clear();
                _ = write!(s, $($arg)*);
                s.as_str()
            }};
        }

        push_sanitized(path, s!("{}", self.artist));
        push_sanitized(path, s!("{} ({})", &self.album, self.year));

        if let Some((n, total)) = self.set
            && total > 1
        {
            s.clear();

            if let Some(media_type) = &self.media_type {
                s.push_str(media_type);
                s.push(' ');
            }

            _ = write!(s, "{n:02}");
            push_sanitized(path, &s);
        }

        push_sanitized(
            path,
            s!(
                "{} - {} - {:02} - {}",
                self.artist,
                self.album,
                self.track,
                &self.title
            ),
        );
    }
}

fn push_sanitized(path: &mut PathBuf, s: &str) {
    path.push(sanitize(s).as_ref());
}

fn sanitize(s: &str) -> Cow<'_, str> {
    let mut out = String::new();

    let rest = 'normalize: {
        for (n, c) in s.char_indices() {
            match c {
                ':' => {
                    out.push_str(&s[..n]);
                    break 'normalize &s[n..];
                }
                c => {
                    if map(c).is_some() {
                        out.push_str(&s[..n]);
                        break 'normalize &s[n..];
                    }
                }
            }
        }

        return Cow::Borrowed(s);
    };

    fn map(c: char) -> Option<&'static str> {
        match c {
            '\\' => Some("+"),
            '/' => Some("+"),
            '<' => Some(""),
            '>' => Some(""),
            '?' => Some(""),
            '*' => Some("-"),
            '|' => Some(""),
            '"' => Some(""),
            _ => None,
        }
    }

    let mut last_whitespace = false;
    let mut it = rest.chars();

    while let Some(c) = it.next() {
        match c {
            ':' => {
                if it.clone().next().is_some_and(|c| c.is_whitespace()) {
                    out.push_str(" - ");
                    it.next();
                } else {
                    out.push('-');
                }
            }
            c => {
                if let Some(repl) = map(c) {
                    out.push_str(repl);
                    continue;
                }

                if last_whitespace && c.is_whitespace() {
                    continue;
                }

                out.push(c);
                last_whitespace = c.is_whitespace();
            }
        }
    }

    Cow::Owned(out)
}

pub(super) struct Dump {
    pub(super) source: Source,
    pub(super) items: Vec<TagItem>,
}

impl Dump {
    pub(crate) fn dump(&self, o: &mut Out<'_>, archives: &Archives) -> Result<()> {
        info!(o, "Tags:");
        let mut o = o.indent(1);

        self.source.dump(&mut o, archives)?;

        for item in &self.items {
            dump_tag_item(&mut o, item)?;
        }

        Ok(())
    }
}

fn dump_tag_item(o: &mut Out<'_>, item: &TagItem) -> Result<()> {
    info!(o, "{:?}:", item.key());
    let mut o = o.indent(1);

    match item.value() {
        ItemValue::Text(text) => {
            blank!(o, "text: {text:?}");
        }
        ItemValue::Locator(link) => {
            blank!(o, "link: {link:?}");
        }
        ItemValue::Binary(data) => {
            blank!(o, "binary: {} bytes", data.len());
        }
    }

    Ok(())
}
