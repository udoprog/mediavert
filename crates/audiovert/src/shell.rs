use core::fmt;

use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

pub(crate) fn path(path: &Path) -> impl fmt::Display + '_ {
    #[repr(transparent)]
    struct Format(OsStr);

    impl Format {
        fn new(s: &OsStr) -> &Self {
            // SAFETY: repr(transparent)
            unsafe { &*(s as *const OsStr as *const Format) }
        }
    }

    impl fmt::Display for Format {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for chunk in self.0.as_encoded_bytes().utf8_chunks() {
                f.write_str(chunk.valid())?;

                for &b in chunk.invalid() {
                    write!(f, "\\u{{{:04x}}}", b)?;
                }
            }

            Ok(())
        }
    }

    Format::new(path.as_os_str())
}

pub(crate) fn escape(s: &OsStr) -> Cow<'_, str> {
    let Some(s) = s.to_str() else {
        return Cow::Borrowed("<non-utf8>");
    };

    escape_str(s)
}

pub(crate) fn escape_str(s: &str) -> Cow<'_, str> {
    let mut o = String::new();

    let s = 'escape: {
        for (n, c) in s.char_indices() {
            if escape_in_bash(c).is_some() {
                o.push_str(&s[..n]);
                break 'escape &s[n..];
            }
        }

        return Cow::Borrowed(s);
    };

    for c in s.chars() {
        if let Some(s) = escape_in_bash(c) {
            o.push_str(s);
        } else {
            o.push(c);
        }
    }

    Cow::Owned(o)
}

pub(crate) fn escape_in_bash(c: char) -> Option<&'static str> {
    match c {
        ' ' => Some("\\ "),
        '"' => Some("\\\""),
        '\'' => Some("\\'"),
        '\\' => Some("\\\\"),
        '$' => Some("\\$"),
        '`' => Some("\\`"),
        '&' => Some("\\&"),
        '|' => Some("\\|"),
        ';' => Some("\\;"),
        '<' => Some("\\<"),
        '>' => Some("\\>"),
        '!' => Some("\\!"),
        '(' => Some("\\("),
        ')' => Some("\\)"),
        '[' => Some("\\["),
        ']' => Some("\\]"),
        _ => None,
    }
}

/// Helper type to format a commands with argument substitutions.
pub(crate) struct FormatCommand<'a> {
    cmd: &'a Command,
    replacements: HashMap<&'a OsStr, Cow<'a, str>>,
}

impl<'a> FormatCommand<'a> {
    pub(crate) fn new(cmd: &'a Command) -> Self {
        Self {
            cmd,
            replacements: HashMap::new(),
        }
    }

    /// Insert a replacement for a given argument.
    pub(crate) fn replace(
        &mut self,
        key: &'a (impl AsRef<OsStr> + ?Sized),
        value: impl Into<Cow<'a, str>>,
    ) {
        self.replacements.insert(key.as_ref(), value.into());
    }
}

impl fmt::Display for FormatCommand<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let program = self.cmd.get_program();

        if let Some(value) = self.replacements.get(program) {
            write!(f, "{value}")?;
        } else {
            write!(f, "{}", escape(program))?;
        }

        for arg in self.cmd.get_args() {
            if let Some(value) = self.replacements.get(arg) {
                write!(f, " {value}")?;
            } else {
                write!(f, " {}", escape(arg))?;
            }
        }

        Ok(())
    }
}
