use core::error::Error;
use core::fmt;
use core::str::FromStr;

use std::process::Command;

use crate::config::Config;

#[derive(Debug)]
pub(crate) struct FormatErr;

impl fmt::Display for FormatErr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported format")
    }
}

impl Error for FormatErr {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Format {
    Aac,
    Flac,
    Mp3,
    Ogg,
    Wav,
}

impl Format {
    pub(crate) const DEFAULT_BITRATE_AAC: u32 = 192;
    pub(crate) const DEFAULT_BITRATE_MP3: u32 = 320;
    pub(crate) const DEFAULT_BITRATE_OGG: u32 = 192;

    pub(crate) fn default_bitrate(&self) -> Option<u32> {
        match self {
            Format::Aac => Some(Format::DEFAULT_BITRATE_AAC),
            Format::Mp3 => Some(Format::DEFAULT_BITRATE_MP3),
            Format::Ogg => Some(Format::DEFAULT_BITRATE_OGG),
            _ => None,
        }
    }

    pub(crate) fn is_lossless(&self) -> bool {
        matches!(self, Format::Flac | Format::Wav)
    }

    pub(crate) fn bitrate(&self, config: &Config, command: &mut Command) {
        if let Some(bitrate) = config.bitrates.get(self)
            && bitrate > 0
        {
            command.arg("-ab");
            command.arg(format!("{bitrate}k"));
        }
    }

    pub(crate) fn ext(&self) -> &'static str {
        match self {
            Format::Aac => "aac",
            Format::Flac => "flac",
            Format::Mp3 => "mp3",
            Format::Ogg => "ogg",
            Format::Wav => "wav",
        }
    }

    pub(crate) fn ffmpeg_format(&self) -> &'static str {
        match self {
            Format::Aac => "adts",
            Format::Flac => "flac",
            Format::Mp3 => "mp3",
            Format::Ogg => "ogg",
            Format::Wav => "wav",
        }
    }

    pub(crate) fn from_ext(ext: &str) -> Option<Format> {
        match ext {
            "aac" => Some(Format::Aac),
            "flac" => Some(Format::Flac),
            "mp3" => Some(Format::Mp3),
            "ogg" => Some(Format::Ogg),
            "wav" => Some(Format::Wav),
            _ => None,
        }
    }
}

impl fmt::Display for Format {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ext().fmt(f)
    }
}

impl FromStr for Format {
    type Err = FormatErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_ext(s).ok_or(FormatErr)
    }
}
