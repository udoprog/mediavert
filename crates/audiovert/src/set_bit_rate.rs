use core::error::Error;
use core::fmt;
use core::str::FromStr;

use crate::condition::{ConditionErr, FromCondition};

#[derive(Debug)]
pub(crate) enum SetBitRateErr {
    MissingSeparator,
    InvalidFromCondition(ConditionErr),
    InvalidBitrate,
}

impl fmt::Display for SetBitRateErr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator => write!(f, "missing '=' separator"),
            Self::InvalidFromCondition(e) => write!(f, "invalid from condition: {e}"),
            Self::InvalidBitrate => write!(f, "invalid bitrate"),
        }
    }
}

impl Error for SetBitRateErr {}

impl From<ConditionErr> for SetBitRateErr {
    #[inline]
    fn from(e: ConditionErr) -> Self {
        SetBitRateErr::InvalidFromCondition(e)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SetBitRate {
    pub(crate) from: FromCondition,
    pub(crate) bitrate: u32,
}

impl FromStr for SetBitRate {
    type Err = SetBitRateErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (from, bitrate) = s.split_once('=').ok_or(SetBitRateErr::MissingSeparator)?;

        Ok(SetBitRate {
            from: from.parse()?,
            bitrate: bitrate.parse().map_err(|_| SetBitRateErr::InvalidBitrate)?,
        })
    }
}

impl fmt::Display for SetBitRate {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.from, self.bitrate)
    }
}
