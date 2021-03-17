use crate::collector::collect_blocks;
use crate::error::TTError;
use crate::utils::FileProxy;
use chrono::{DateTime, Local, NaiveTime};
use std::io;
use std::io::{BufRead, Write};

// interface for the runner, adds what is in the opt to the logfile
// public interface is the function add()
// interface for the runner, adds what is in the opt to the logfile
// public interface is the function add()
pub(crate) fn run<R: BufRead, W: Write, F: FileProxy<R, W>>(
    now: &DateTime<Local>,
    default_logfile: &F,
    _activitiesfile: &F,
) -> Result<i32, TTError> {
    match is_active(&now.naive_local().time(), default_logfile.reader()?.lines())? {
        false => Ok(1),
        true => Ok(0),
    }
}
pub fn is_active<T: Iterator<Item = io::Result<String>>>(
    add_endit_at: &NaiveTime,
    logfile: T,
) -> Result<bool, TTError> {
    let collected = collect_blocks(logfile, Some(add_endit_at))?;
    match collected {
        None => Ok(false),
        Some(collected) if collected.final_activity == "break" => Ok(false),
        _ => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::setup_line_reader;
    use std::iter::FromIterator;

    #[test]
    fn end_with_break() {
        let lines = setup_line_reader(vec!["8:30 start", "10:00 break"]);
        assert!(!is_active(&NaiveTime::from_hms(11, 0, 0), lines).unwrap());
    }

    #[test]
    fn end_not_with_break() {
        let lines = setup_line_reader(vec!["8:30 start"]);
        assert!(is_active(&NaiveTime::from_hms(11, 0, 0), lines).unwrap());
    }

    #[test]
    fn empty() {
        let lines = setup_line_reader(vec![]);
        assert!(!is_active(&NaiveTime::from_hms(11, 0, 0), lines).unwrap());
    }
}
