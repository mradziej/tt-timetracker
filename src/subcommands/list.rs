use crate::error::TTError;
use crate::utils::FileProxy;
use chrono::{DateTime, Local};
use std::io::{BufRead, Write};

pub fn run<'a, R: BufRead, W: Write, F: FileProxy<R, W>>(
    _now: &DateTime<Local>,
    _default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    list_activities(activitiesfile.reader()?);
    Ok(0)
}

pub fn list_activities(activitiesfile: impl BufRead) {
    for line in activitiesfile.lines() {
        println!("{}", line.unwrap());
    }
}
