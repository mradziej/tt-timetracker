use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::result::Result;
use std::str;

use chrono::DateTime;
use chrono::Local;
use chrono::NaiveDate;
use structopt::StructOpt;

use crate::error::TTError;
use crate::utils::FileProxy;

use self::subcommands::add::AddOpt;
use self::subcommands::report::ReportOpt;
use crate::configfile::TTConfig;
use crate::subcommands::edit::EditOpt;
use crate::subcommands::resume::ResumeOpt;

pub mod collector;
pub mod configfile;
pub mod error;
pub mod log_parser;
pub mod subcommands;
pub mod utils;

#[cfg(test)]
mod tests;

#[derive(StructOpt, Debug)]
#[structopt(name = "tt", about = "a command line time tracker")]
struct Opt {
    #[structopt(subcommand)]
    command: Subcommand,
}

impl Opt {
    pub fn from(subcommand: Subcommand) -> Self {
        Self {
            command: subcommand,
        }
    }
}

#[derive(StructOpt, Debug)]
enum Subcommand {
    /// add activities to the log file
    Add(AddOpt),

    /// edit the log file or activities file
    Edit(EditOpt),

    /// create a report
    Report(ReportOpt),

    /// resume the activity before the current one
    Resume(ResumeOpt),

    /// list all activities from the activities file
    List,

    /// enter interactive mode
    Interactive,

    /// is there currently an activity ongoing? For scripts.
    IsActive,

    /// watch focus of i3 workspaces and configure their names or log activities
    WatchI3,
}

pub fn get_activities_file_name() -> PathBuf {
    let mut path =
        dirs::home_dir().expect("Cannot figure out your home directory. What's wrong with you?");
    path.push(".tt");
    path.push("activities");
    path
}

pub fn get_logfile_name(date: &NaiveDate) -> PathBuf {
    let mut path =
        dirs::home_dir().expect("Cannot figure out your home directory. What's wrong with you?");
    let date_str = format!("{}", date.format("%F"));
    path.push(".tt");
    path.push(date_str);
    path
}

pub fn get_configfile_name() -> PathBuf {
    let mut path =
        dirs::home_dir().expect("Cannot figure out your home directory. What's wrong with you?");
    path.push(".tt");
    path.push("config.toml");
    path
}

/////////////////////////////////////////////////////

pub fn run<R: BufRead, W: Write, F: FileProxy<R, W>>(
    args: &Vec<String>,
    now: &DateTime<Local>,
    configfile: &F,
    default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    // we need to hold the config_writer to keep the settings thread safe for tests.
    let _config_writer = TTConfig::init(configfile.reader()?)?;

    let opt = match args.get(1).map(String::as_str) {
        None => Opt::from(Subcommand::Interactive),
        Some("-y") => Opt::from(Subcommand::Report(ReportOpt::from_iter(args))),
        Some("add") | Some("report") | Some("list") | Some("edit") | Some("resume")
        | Some("is-active") | Some("watch-i3") | Some("help") | Some("--help") | Some("-h") => {
            Opt::from_iter(args)
        }
        Some(_) => Opt::from(Subcommand::Add(AddOpt::from_iter(args))),
    };
    match opt.command {
        Subcommand::Add(add_opt) => {
            subcommands::add::run(add_opt, now, default_logfile, activitiesfile)
        }
        Subcommand::Edit(edit_opt) => {
            subcommands::edit::run(edit_opt, now, default_logfile, activitiesfile)
        }
        Subcommand::Interactive => {
            subcommands::interactive::run(now, default_logfile, activitiesfile)
        }
        Subcommand::List => subcommands::list::run(now, default_logfile, activitiesfile),
        Subcommand::Report(report_opt) => {
            subcommands::report::run(report_opt, now, default_logfile, activitiesfile)
        }
        Subcommand::Resume(resume_opt) => {
            subcommands::resume::run(resume_opt, now, default_logfile, activitiesfile)
        }
        Subcommand::IsActive => subcommands::is_active::run(now, default_logfile, activitiesfile),
        Subcommand::WatchI3 => subcommands::watch_i3::run(now, default_logfile, activitiesfile),
    }
}

// let opt = match args.get(1).map(String::as_str) {
//     None => Opt::Interactive,
//     Some("-y") => Opt::Report(ReportOpt::from_iter(args)),
//     Some("add") | Some("report") | Some("list") | Some("edit") | Some("resume") => {
//         Opt::from_iter(args)
//     }
//     Some(_) => Opt::Add(AddOpt::from_iter(args)),
// };
//
// let result = match opt {
//     Opt::Add(add_opt) => {
//         let activity_map = configfile::read_actitivies(activitiesfile.reader()?)
//             .expect("Cannot read config file");
//         log_adder::write_log(
//             add_opt,
//             &activity_map,
//             activitiesfile,
//             default_logfile,
//             &now.time(),
//         )
//         .unwrap();
//         reporter::report(
//             default_logfile.reader()?,
//             &SummaryFormat::Short,
//             Some(&now.naive_local().time()),
//             &None,
//         )
//     }
//     Opt::Report(report_opt) => {
//         let (logfile_reader, add_ending_at) = match (report_opt.date, report_opt.yesterday) {
//             (None, false) => (default_logfile.reader(), Some(now.naive_local().time())),
//             (None, true) => (
//                 F::new(get_logfile_name(&now.date().pred().naive_local())).reader(),
//                 None,
//             ),
//             (Some(date), false) => (F::new(get_logfile_name(&date)).reader(), None),
//             (Some(_), true) => panic!(
//                 "You can either specify a --date or --yesterday, but you specified both."
//             ),
//         };
//         reporter::report(
//             logfile_reader?,
//             &report_opt.format.unwrap_or(SummaryFormat::Long),
//             add_ending_at.as_ref(),
//             &report_opt.cutoff,
//         )
//     }
//     Opt::List => list_activities(activitiesfile.reader()?),
//     }
//     Opt::Resume(resume_opt) => {
//         log_adder::resume(activitiesfile, default_logfile, &timestamp, &resume_opt)
//     }
//     Opt::Interactive => interactive(activitiesfile, default_logfile, &timestamp),
// };
// if let Err(err) = result {
//     println!("{}", err);
//     // return Err(Box::new(err));
// }
// Ok(())
