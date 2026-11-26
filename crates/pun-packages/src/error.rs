use std::fmt::{self, Display};

#[derive(Debug)]
pub enum Error {
    Message(String),
    Context {
        message: String,
        source: Box<Error>,
    },
    /// A script that exited with `code`; Composer exits with that code too.
    Script {
        message: String,
        code: i32,
    },
    Io(std::io::Error),
    Json(serde_json::Error),
    Pcre(pcre2::Error),
    Http(ureq::Error),
    Zip(zip::result::ZipError),
    ParseInt(std::num::ParseIntError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn msg(message: impl Display) -> Error {
        Error::Message(message.to_string())
    }

    /// The exit code of the failed script behind this error, through any context around it.
    pub fn script_code(&self) -> Option<i32> {
        match self {
            Error::Script { code, .. } => Some(*code),
            Error::Context { source, .. } => source.script_code(),
            _ => None,
        }
    }

    fn head(&self) -> &dyn Display {
        match self {
            Error::Message(m)
            | Error::Context { message: m, .. }
            | Error::Script { message: m, .. } => m,
            Error::Io(e) => e,
            Error::Json(e) => e,
            Error::Pcre(e) => e,
            Error::Http(e) => e,
            Error::Zip(e) => e,
            Error::ParseInt(e) => e,
        }
    }
}

impl Display for Error {
    /// `{:#}` appends each cause after `: `, as anyhow does.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.head())?;
        if f.alternate() {
            let mut cause = std::error::Error::source(self);
            while let Some(e) = cause {
                write!(f, ": {e}")?;
                cause = e.source();
            }
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Message(_) | Error::Script { .. } => None,
            Error::Context { source, .. } => Some(source.as_ref()),
            Error::Io(e) => e.source(),
            Error::Json(e) => e.source(),
            Error::Pcre(e) => e.source(),
            Error::Http(e) => e.source(),
            Error::Zip(e) => e.source(),
            Error::ParseInt(e) => e.source(),
        }
    }
}

macro_rules! from {
    ($($variant:ident($type:ty)),* $(,)?) => {
        $(impl From<$type> for Error {
            fn from(e: $type) -> Error {
                Error::$variant(e)
            }
        })*
    };
}

from!(
    Io(std::io::Error),
    Json(serde_json::Error),
    Pcre(pcre2::Error),
    Http(ureq::Error),
    Zip(zip::result::ZipError),
    ParseInt(std::num::ParseIntError),
);

pub trait Context<T> {
    fn context(self, message: impl Display) -> Result<T>;
    fn with_context<M: Display>(self, message: impl FnOnce() -> M) -> Result<T>;
}

impl<T, E: Into<Error>> Context<T> for std::result::Result<T, E> {
    fn context(self, message: impl Display) -> Result<T> {
        self.with_context(|| message)
    }

    fn with_context<M: Display>(self, message: impl FnOnce() -> M) -> Result<T> {
        self.map_err(|e| Error::Context {
            message: message().to_string(),
            source: Box::new(e.into()),
        })
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, message: impl Display) -> Result<T> {
        self.ok_or_else(|| Error::msg(message))
    }

    fn with_context<M: Display>(self, message: impl FnOnce() -> M) -> Result<T> {
        self.ok_or_else(|| Error::msg(message()))
    }
}

macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::Error::Message(format!($($arg)*)))
    };
}
pub(crate) use bail;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    fn same<E: std::error::Error + Into<Error>>(make: impl Fn() -> E) {
        let (wrapped, plain) = (make().into(), make());
        assert_eq!(wrapped.to_string(), plain.to_string());
        assert_eq!(
            wrapped.source().map(ToString::to_string),
            plain.source().map(ToString::to_string)
        );
    }

    #[test]
    fn wrapped_errors_print_as_themselves() {
        same(|| std::io::Error::other("gone"));
        same(|| serde_json::from_str::<i64>("x").unwrap_err());
        same(|| pcre2::bytes::Regex::new("(").unwrap_err());
        same(|| ureq::Error::Io(std::io::Error::other("refused")));
        same(|| zip::ZipArchive::new(std::io::Cursor::new(b"")).unwrap_err());
        same(|| "x".parse::<i64>().unwrap_err());
    }

    #[test]
    fn alternate_prints_every_cause_as_anyhow() {
        let io: std::result::Result<(), _> = Err(std::io::Error::other("gone"));
        let err = io.context("reading").context("installing").unwrap_err();
        assert_eq!(err.to_string(), "installing");
        assert_eq!(format!("{err:#}"), "installing: reading: gone");
        assert_eq!(format!("{:#}", Error::msg("plain")), "plain");
        assert_eq!(Error::msg("plain").script_code(), None);
        let script: std::result::Result<(), _> = Err(Error::Script {
            message: "failed".into(),
            code: 3,
        });
        let err = script.context("called").unwrap_err();
        assert_eq!(
            (format!("{err:#}"), err.script_code()),
            ("called: failed".into(), Some(3))
        );
    }
}
