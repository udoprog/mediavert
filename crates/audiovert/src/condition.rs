use core::error::Error;
use core::fmt;
use core::str::FromStr;

use crate::bitrates::Bitrates;
use crate::format::{Format, FormatErr};

#[derive(Debug)]
pub(crate) enum ConditionErr {
    Format(FormatErr),
}

impl fmt::Display for ConditionErr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConditionErr::Format(err) => err.fmt(f),
        }
    }
}

impl From<FormatErr> for ConditionErr {
    #[inline]
    fn from(err: FormatErr) -> Self {
        ConditionErr::Format(err)
    }
}

impl Error for ConditionErr {}

#[derive(Copy, Clone, Debug)]
pub(crate) enum FromCondition {
    Lossless,
    Lossy,
    Exact(Format),
}

impl FromCondition {
    pub(crate) fn pick_bitrates<'bits>(
        &self,
        bitrates: &'bits mut Bitrates,
    ) -> impl Iterator<Item = (Format, &'bits mut u32)> + 'bits {
        let this = *self;
        bitrates
            .iter_mut()
            .filter(move |(format, _)| this.matches(*format))
    }

    pub(crate) fn matches(self, format: Format) -> bool {
        match self {
            FromCondition::Lossless => format.is_lossless(),
            FromCondition::Lossy => !format.is_lossless(),
            FromCondition::Exact(f) => f == format,
        }
    }
}

impl FromStr for FromCondition {
    type Err = ConditionErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "lossless" => Ok(Self::Lossless),
            "lossy" => Ok(Self::Lossy),
            _ => Ok(Self::Exact(s.parse()?)),
        }
    }
}

impl fmt::Display for FromCondition {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FromCondition::Lossless => write!(f, "lossless"),
            FromCondition::Lossy => write!(f, "lossy"),
            FromCondition::Exact(format) => format.fmt(f),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum ToCondition {
    Exact(Format),
    Same,
}

impl ToCondition {
    #[inline]
    pub(crate) fn to_format(self, format: Format) -> Format {
        match self {
            ToCondition::Exact(f) => f,
            ToCondition::Same => format,
        }
    }
}

impl FromStr for ToCondition {
    type Err = ConditionErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "same" => Ok(ToCondition::Same),
            _ => Ok(ToCondition::Exact(s.parse()?)),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum Condition {
    Same,
    FromTo {
        from: FromCondition,
        to: ToCondition,
    },
    To {
        to: ToCondition,
    },
}

impl Condition {
    #[inline]
    pub(crate) fn to_format(self, format: Format) -> Option<Format> {
        match self {
            Condition::Same => Some(format),
            Condition::To { to } => Some(to.to_format(format)),
            Condition::FromTo { from, to } => {
                if from.matches(format) {
                    Some(to.to_format(format))
                } else {
                    None
                }
            }
        }
    }
}

impl FromStr for Condition {
    type Err = ConditionErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "same" => Ok(Condition::Same),
            _ => {
                let Some((from, to)) = s.split_once('=') else {
                    return Ok(Condition::To { to: s.parse()? });
                };

                Ok(Condition::FromTo {
                    from: from.parse()?,
                    to: to.parse()?,
                })
            }
        }
    }
}
