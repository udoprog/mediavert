use std::collections::HashMap;

use crate::format::Format;

const DEFAULT_BITRATES: [(Format, u32); 3] = [
    (Format::Aac, Format::DEFAULT_BITRATE_AAC),
    (Format::Mp3, Format::DEFAULT_BITRATE_MP3),
    (Format::Ogg, Format::DEFAULT_BITRATE_OGG),
];

pub(crate) struct Bitrates {
    map: HashMap<Format, u32>,
}

impl Bitrates {
    #[inline]
    pub(crate) fn get(&self, format: &Format) -> Option<u32> {
        Some(*self.map.get(format)?)
    }

    #[inline]
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (Format, &mut u32)> + '_ {
        self.map.iter_mut().map(|(f, v)| (*f, v))
    }
}

impl Default for Bitrates {
    #[inline]
    fn default() -> Self {
        Self {
            map: HashMap::from(DEFAULT_BITRATES),
        }
    }
}
