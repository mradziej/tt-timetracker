use itertools::Itertools;
use std::error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub struct TTError {
    pub kind: TTErrorKind,
    pub context: Vec<String>,
}

#[derive(Debug)]
pub enum TTErrorKind {
    IoError(io::Error),
    UsageError(&'static str),
    // message, line
    ActivityConfigError(&'static str),
    ParseError(&'static str, String),
    JsonError(serde_json::Error),
}

impl TTError {
    pub fn new(kind: TTErrorKind) -> TTError {
        TTError {
            kind,
            context: vec![],
        }
    }

    pub fn context(self, str: String) -> TTError {
        let TTError { kind, mut context } = self;
        context.push(str);
        TTError { kind, context }
    }
}

impl fmt::Display for TTError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TTErrorKind::IoError(err) => err.fmt(f),
            TTErrorKind::UsageError(s) => write!(f, "Usage error: {}", s),
            TTErrorKind::ActivityConfigError(s) => write!(f, "activity file error: {}", s),
            TTErrorKind::ParseError(msg, line) => write!(f, "parse error: {} at: {}", msg, line),
            TTErrorKind::JsonError(err) => write!(f, "json parse error: {}", err),
        }?;
        if !self.context.is_empty() {
            write!(f, "\n")?;
        }
        let context = self
            .context
            .iter()
            .map(|s| format!("while {}", s))
            .join("\n");
        write!(f, "{}", context)
    }
}

impl error::Error for TTError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl std::convert::From<TTErrorKind> for TTError {
    fn from(kind: TTErrorKind) -> Self {
        TTError::new(kind)
    }
}

impl std::convert::From<std::io::Error> for TTError {
    fn from(error: std::io::Error) -> Self {
        TTError::new(TTErrorKind::IoError(error))
    }
}

impl std::convert::From<serde_json::Error> for TTError {
    fn from(error: serde_json::Error) -> Self {
        TTError::new(TTErrorKind::JsonError(error))
    }
}

// #[derive(Debug)]
// pub struct WrappedParseError {}
// impl fmt::Display for WrappedParseError {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "Parse Error")
//     }
// }
//
// impl Error for WrappedParseError {}
//
// impl std::convert::From<std::io::Error> for ParseError {
//     fn from(error: std::io::Error) -> Self {
//         ParseError::IoError(error)
//     }
// }
//
// impl fmt::Display for ParseError {
//     fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
//         match self {
//             ParseError::ParseError(msg, line) => {
//                 write!(fmt, "parsing error in this line: `{}`:\n{}", line, msg)
//             }
//             ParseError::IoError(err) => err.fmt(fmt),
//         }
//     }
// }
//
// impl Error for ParseError {
//     fn source(&self) -> Option<&(dyn Error + 'static)> {
//         match self {
//             ParseError::IoError(err) => err.source(),
//             ParseError::ParseError(_msg, _line) => Some(&WrappedParseError {}),
//         }
//     }
// }
