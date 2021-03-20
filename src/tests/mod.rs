mod test_add;
mod test_log_parser;

use crate::error::TTError;
use crate::utils::{FakeFile, FileProxy};
use chrono::prelude::*;
use std::io::Write;
use std::path::PathBuf;

pub struct TestData {
    pub now: DateTime<Local>,
    pub configfile: FakeFile,
    pub logfile: FakeFile,
    pub activitiesfile: FakeFile,
    pub args: String,
}

impl TestData {
    pub fn new() -> Self {
        TestData {
            now: Local.ymd(2020, 6, 1).and_hms(8, 0, 0),
            configfile: FakeFile::new(PathBuf::from("config")),
            logfile: FakeFile::new(PathBuf::from("logfile")),
            activitiesfile: FakeFile::new(PathBuf::from("configfile")),
            args: "timetracker".to_string(),
        }
    }

    pub fn run(self) -> Result<Self, TTError> {
        let mut args = vec!["timetracker".to_string()];
        args.extend(self.args.split_whitespace().map(|s| s.to_string()));
        crate::run(
            &args,
            &self.now,
            &self.configfile,
            &self.logfile,
            &self.activitiesfile,
        )
        .map(|_| self)
    }

    pub fn with_args(mut self, args: &str) -> Self {
        self.args = args.to_string();
        self
    }

    #[allow(unused)]
    pub fn with_now(mut self, now: DateTime<Local>) -> Self {
        self.now = now;
        self
    }

    pub fn write_configfile(self, configfile: String) -> Self {
        write!(self.configfile.writer().unwrap(), "{}", configfile).unwrap();
        self
    }

    #[allow(unused)]
    pub fn write_logfile(self, configfile: String) -> Self {
        write!(self.logfile.writer().unwrap(), "{}", configfile).unwrap();
        self
    }

    pub fn write_activitiesfile(self, configfile: String) -> Self {
        write!(self.activitiesfile.writer().unwrap(), "{}", configfile).unwrap();
        self
    }
}
