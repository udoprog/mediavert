use core::cell::Cell;
use core::fmt;

use std::io;

use termcolor::ColorSpec;
use termcolor::WriteColor;

macro_rules! __log {
    ($log:ident, $o:ident $(, $($tt:tt)*)?) => {
        $( $o.$log(format_args!($($tt)*))?; )*
    };
}

pub(crate) use __log;

macro_rules! __blank { ($($tt:tt)*) => { $crate::out::__log!(blank, $($tt)*) }; }
macro_rules! __info { ($($tt:tt)*) => { $crate::out::__log!(info, $($tt)*) }; }
macro_rules! __warn { ($($tt:tt)*) => { $crate::out::__log!(warn, $($tt)*) }; }
macro_rules! __error { ($($tt:tt)*) => { $crate::out::__log!(error, $($tt)*) }; }

pub(crate) use __blank as blank;
pub(crate) use __error as error;
pub(crate) use __info as info;
pub(crate) use __warn as warn;

pub(crate) struct Colors {
    info: ColorSpec,
    warn: ColorSpec,
    error: ColorSpec,
}

impl Colors {
    pub(crate) fn new() -> Self {
        let mut info = ColorSpec::new();
        info.set_fg(Some(termcolor::Color::Green)).set_bold(true);

        let mut warn = ColorSpec::new();
        warn.set_fg(Some(termcolor::Color::Yellow)).set_bold(true);

        let mut error = ColorSpec::new();
        error.set_fg(Some(termcolor::Color::Red)).set_bold(true);

        Colors { info, warn, error }
    }
}

pub(crate) struct Out<'a> {
    change: isize,
    indent: &'a Cell<usize>,
    c: &'a Colors,
    o: &'a mut dyn WriteColor,
}

impl Out<'_> {
    pub(crate) fn new<'a>(
        indent: &'a Cell<usize>,
        c: &'a Colors,
        o: &'a mut dyn WriteColor,
    ) -> Out<'a> {
        Out {
            change: 0,
            indent,
            c,
            o,
        }
    }
}

impl<'a> Out<'a> {
    pub(crate) fn indent(&mut self, change: isize) -> Out<'_> {
        let indent = self.indent.get().saturating_add_signed(change);
        self.indent.set(indent);

        Out {
            change,
            indent: self.indent,
            c: self.c,
            o: self.o,
        }
    }

    pub(crate) fn blank(&mut self, m: impl fmt::Display) -> io::Result<()> {
        self.prefix()?;
        writeln!(self.o, "{m}")?;
        self.o.flush()?;
        Ok(())
    }

    pub(crate) fn info(&mut self, m: impl fmt::Display) -> io::Result<()> {
        self.colorize(&self.c.info, m)
    }

    pub(crate) fn warn(&mut self, m: impl fmt::Display) -> io::Result<()> {
        self.colorize(&self.c.warn, m)
    }

    pub(crate) fn error(&mut self, m: impl fmt::Display) -> io::Result<()> {
        self.colorize(&self.c.error, m)
    }

    fn prefix(&mut self) -> io::Result<()> {
        let n = self.indent.get();

        for _ in 0..n {
            self.o.write_all(b"  ")?;
        }

        Ok(())
    }

    fn colorize(&mut self, c: &ColorSpec, m: impl fmt::Display) -> io::Result<()> {
        self.prefix()?;
        self.o.set_color(c)?;
        writeln!(self.o, "{m}")?;
        self.o.reset()?;
        self.o.flush()?;
        Ok(())
    }
}

impl Drop for Out<'_> {
    #[inline]
    fn drop(&mut self) {
        let indent = self.indent.get().saturating_sub_signed(self.change);
        self.indent.set(indent);
    }
}
