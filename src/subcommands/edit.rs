use crate::error::{TTError, TTErrorKind};
use crate::get_logfile_name;
use crate::utils::FileProxy;
use chrono::{DateTime, Local, NaiveDate};
use std::io::{BufRead, Write};
use std::path::Path;
use std::{env, process};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct EditOpt {
    #[structopt(long)]
    /// edit log for this day (default is today), only for reporting
    pub date: Option<NaiveDate>,

    #[structopt(short, long)]
    /// edit yesterday's log
    pub yesterday: bool,

    #[structopt(short, long)]
    /// edit the activities file
    pub activities: bool,
}

// interface for the runner, adds what is in the opt to the logfile
// public interface is the function add()
pub(crate) fn run<'a, R: BufRead, W: Write, F: FileProxy<R, W>>(
    edit_opt: EditOpt,
    now: &DateTime<Local>,
    default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    let pathname = match (edit_opt.activities, edit_opt.date, edit_opt.yesterday) {
        (true, None, false) => Ok(activitiesfile.pathname().to_path_buf()),
        (true, _, _) => Err(TTError::new(TTErrorKind::UsageError(
            "edit --activity does not allow dates",
        ))),
        (false, Some(date), false) => Ok(get_logfile_name(&date)),
        (false, Some(_data), true) => Err(TTError::new(TTErrorKind::UsageError(
            "You can either specify a --date or --yesterday, but you specified both.",
        ))),
        (false, None, false) => Ok(default_logfile.pathname().to_path_buf()),
        (false, None, true) => Ok(get_logfile_name(&now.date().pred().naive_local())),
    };
    pathname.and_then(|p| edit(&p))?;
    Ok(0)
}

// start the editor for the given file (uses the environment variable $EDITOR)
pub fn edit(filename: &Path) -> Result<(), TTError> {
    process::Command::new(env::var("EDITOR").unwrap_or_else(|_| "vi".to_string()))
        .arg(filename)
        .spawn()
        .expect("could not start editor")
        .wait()
        .expect("could not wait for termination of editor");
    Ok(())
}
