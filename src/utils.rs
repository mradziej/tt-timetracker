use std::cell::RefCell;
use std::io::{self, BufReader, BufWriter, Chain, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::result::Result;
use std::string::FromUtf8Error;
use std::{
    fs::{File, OpenOptions},
    str,
};

use crate::configfile::TTConfig;
use chrono::Duration;
use chrono::NaiveTime;
use chrono::Timelike;

// use crate::SummaryFormat::{Short, Long};

/// parses the time as HH:MM
/// ```
/// use timetracker::utils;
/// use chrono::NaiveTime;
/// assert_eq!(utils::parse_time("15:23"), Ok(NaiveTime::from_hms(15,23,00)))
/// ```
pub fn parse_time(s: &str) -> Result<NaiveTime, chrono::format::ParseError> {
    NaiveTime::parse_from_str(s, "%H:%M")
}

/// formats time as HH:MM
/// ```
/// use timetracker::utils;
/// use chrono::NaiveTime;
/// assert_eq!(format!("{}", utils::format_time(&NaiveTime::from_hms(15,23,00))), "15:23");
/// ```
pub fn format_time(
    t: &NaiveTime,
) -> chrono::format::DelayedFormat<chrono::format::strftime::StrftimeItems> {
    t.format("%H:%M")
}

/// parses a duration from HH:MM
/// ```
/// use timetracker::utils;
/// use chrono::Duration;
/// assert_eq!(utils::parse_duration("01:54").map(|d| d.num_minutes()), Ok(60+54));
/// ```
pub fn parse_duration(s: &str) -> Result<Duration, chrono::format::ParseError> {
    NaiveTime::parse_from_str(s, "%H:%M")
        .map(|t| Duration::seconds(t.num_seconds_from_midnight() as i64))
}

/// formats a duration as HH:MM
/// ```
/// use timetracker::utils;
/// use chrono::Duration;
/// assert_eq!(format!("{}", utils::format_duration(&Duration::minutes(90))), "1:30");
/// ```
pub fn format_duration(duration: &chrono::Duration) -> String {
    let mins = ((duration.num_seconds() + 30) / 60) as u16;
    let hours = (mins / 60) as u16;
    format!("{}:{:02}", hours, mins - hours * 60)
}

/// if the activity is just a bare jira ticket number, return a proper jira ticket ID
/// ```
/// use timetracker::{utils, configfile};
/// use timetracker::utils::{FakeFile, FileProxy};
/// use timetracker::configfile::TTConfig;
/// use std::io::Write;
/// use std::path::PathBuf;
/// let configfile = FakeFile::new(PathBuf::from("configfile"));
/// configfile.writer().unwrap().write("prefix = \"JIRAPROJECT\"".as_bytes());
/// TTConfig::init(configfile.reader().unwrap()).unwrap();
/// assert_eq!(utils::resolve_prefix_for_number("234"), "JIRAPROJECT-234");
/// ```
pub fn resolve_prefix_for_number(activity: &str) -> String {
    match activity.chars().next() {
        Some('0'..='9') => match &TTConfig::get().prefix {
            Some(prefix) => format!("{}-{}", prefix, activity),
            None => activity.to_string(),
        },
        _ => activity.to_string(),
    }
}

pub trait FileProxy<R: Read, W: Write> {
    fn pathname(&self) -> &Path;
    fn reader(&self) -> io::Result<R>;
    fn writer(&self) -> io::Result<W>;
    fn new(pathname: PathBuf) -> Self;
}

pub struct NamedFile {
    pathname: PathBuf,
}

// trait BufferedFileProxy = FileProxy<BufReader<File>, BufWriter<File>>;

impl FileProxy<BufReader<File>, BufWriter<File>> for NamedFile {
    fn pathname(&self) -> &Path {
        self.pathname.as_ref()
    }
    fn reader(&self) -> io::Result<BufReader<File>> {
        match File::open(self.pathname()) {
            Ok(f) => Ok(BufReader::new(f)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                File::open("/dev/null").map(BufReader::new)
            }
            Err(err) => Err(err),
        }
    }
    fn writer(&self) -> io::Result<BufWriter<File>> {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.pathname())
            .map(BufWriter::new)
    }
    fn new(pathname: PathBuf) -> NamedFile {
        NamedFile { pathname }
    }
}

pub struct RcBuffer {
    buffer: Rc<RefCell<Vec<u8>>>,
}

impl Write for RcBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut clone = self.buffer.borrow().clone();
        let res = clone.write(buf);
        self.buffer.replace(clone);
        res
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut clone = self.buffer.borrow().clone();
        let res = clone.flush();
        self.buffer.replace(clone);
        res
    }
}

impl RcBuffer {
    fn new() -> RcBuffer {
        RcBuffer {
            buffer: Rc::new(RefCell::new(Vec::new())),
        }
    }
    fn clone(&self) -> RcBuffer {
        RcBuffer {
            buffer: self.buffer.clone(),
        }
    }
    fn close(self) -> Vec<u8> {
        let buffer = self.buffer.borrow();
        buffer.clone()
    }
}

pub struct FakeFile {
    pathname: PathBuf,
    source: &'static [u8],
    sink: RcBuffer,
}

// type FakeReader = Chain<BufReader<&'static [u8]>, Cursor<Vec<u8>>>;
type FakeWriter = BufWriter<RcBuffer>;
type ReadChain = Chain<BufReader<&'static [u8]>, Cursor<Vec<u8>>>;
impl FileProxy<Chain<BufReader<&'static [u8]>, Cursor<Vec<u8>>>, FakeWriter> for FakeFile {
    fn pathname(&self) -> &Path {
        self.pathname.as_ref()
    }
    fn reader(&self) -> io::Result<ReadChain> {
        let read_source = BufReader::new(self.source);
        let read_buffer = Cursor::new(self.sink.clone().close());
        Ok(read_source.chain(read_buffer))
    }
    fn writer(&self) -> io::Result<FakeWriter> {
        Ok(BufWriter::new(self.sink.clone()))
    }
    fn new(pathname: PathBuf) -> FakeFile {
        FakeFile::with_content(pathname, b"")
    }
}

impl FakeFile {
    //noinspection RsExternalLinter,RsExternalLinter
    pub(crate) fn close(self) -> Result<String, FromUtf8Error> {
        Ok(String::from_utf8(self.source.to_vec())?
            + String::from_utf8(self.sink.close())?.as_ref())
    }
    pub(crate) fn with_content(pathname: PathBuf, content: &'static [u8]) -> FakeFile {
        FakeFile {
            pathname,
            source: content,
            sink: RcBuffer::new(),
        }
    }
}

#[allow(unused)]
pub(crate) fn setup_line_reader(
    lines: Vec<&'static str>,
) -> Box<dyn Iterator<Item = Result<String, io::Error>>> {
    Box::new(lines.into_iter().map(|s| Ok(s.to_string())))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn write_and_read() {
        let expected = "\
Das Pferd frisst keinen grünen Gurkensalat.
Auch keinen roten Tomatensalat.
Peter mag keine Petersilie.
Karl liebt Kartoffeln.\n";
        let logfile = FakeFile::with_content(
            PathBuf::from("fakefile"),
            "Das Pferd frisst keinen grünen Gurkensalat.\n".as_bytes(),
        );
        {
            let mut writer = logfile.writer().unwrap();
            assert!(writeln!(writer, "Auch keinen roten Tomatensalat.").is_ok());
            assert!(writer.flush().is_ok());
            let mut writer2 = logfile.writer().unwrap();
            assert!(writeln!(writer2, "Peter mag keine Petersilie.").is_ok());
            assert!(writer2.flush().is_ok());
            assert!(writeln!(writer, "Karl liebt Kartoffeln.").is_ok());
        }
        let mut reader = logfile.reader().unwrap();
        let mut buffer = String::new();
        assert!(reader.read_to_string(&mut buffer).is_ok());
        assert_eq!(buffer, expected);

        assert_eq!(logfile.close().unwrap(), expected);
    }

    #[test]
    fn format_duration_almost_one_hour() {
        assert_eq!(format_duration(&Duration::milliseconds(3599999)), "1:00");
    }
}
