use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::result::Result;
use std::string::String;

use chrono::{self, DateTime, Duration, Local, NaiveTime};
use itertools::Itertools;
use structopt::StructOpt;

use crate::collector::collect_blocks;
use crate::error::{TTError, TTErrorKind};
use crate::log_parser::{is_break, is_distributable, is_start};
use crate::log_parser::{Block, BlockData};
use crate::subcommands::report::{report, SummaryFormat};
use crate::utils;
use crate::utils::FileProxy;

#[derive(StructOpt, Debug)]
pub(crate) struct AddOpt {
    #[structopt(short, long)]
    /// correct activity or timestamp of the previous log entry
    pub really: bool,

    #[structopt(short, long="time", parse(try_from_str = utils::parse_time))]
    /// use this time instead of the current time; format: HH:MM
    pub timestamp: Option<NaiveTime>,

    #[structopt(short, long, default_value)]
    /// activity started so many minutes before
    pub ago: i64,

    #[structopt(name = "ACTIVITY")]
    /// which activity (start with "+" if not in activities file)
    pub activity: Option<String>,

    #[structopt(name = "tags")]
    /// any additional tags; start first tag with "=" to give a shortname
    pub tags: Vec<String>,
}

// interface for the runner, adds what is in the opt to the logfile
// public interface is the function add()
pub(crate) fn run<'a, R: BufRead, W: Write, F: FileProxy<R, W>>(
    opt: AddOpt,
    now: &DateTime<Local>,
    default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    let timestamp = now.time();
    let block = block_from_opt(opt, &timestamp)?;
    let activity_map = read_activities(activitiesfile.reader()?)?;
    add(
        block,
        Some(&activity_map),
        activitiesfile,
        default_logfile,
        &timestamp,
        now,
    )?;
    Ok(0)
}

// converts the options to a Block that represents a log entry
fn block_from_opt(opt: AddOpt, logtime: &NaiveTime) -> Result<Block, TTError> {
    let distribute = &opt.activity.as_ref().map(|a| is_distributable(a));
    match opt {
        //  (opt.really, opt.activity, opt.timestamp, opt.tags)
        AddOpt {
            really: true,
            activity: None,
            timestamp: Some(real_timestamp),
            tags,
            ago: before,
        } if tags.is_empty() => Ok(Block::TimeCorrection(
            real_timestamp - Duration::minutes(before),
        )),
        AddOpt {
            really: true,
            activity: None,
            timestamp: None,
            tags,
            ago: before,
        } if before > 0 && tags.is_empty() => {
            Ok(Block::TimeCorrection(*logtime - Duration::minutes(before)))
        }
        AddOpt {
            really: true,
            activity: Some(real_activity),
            timestamp: None,
            tags,
            ago: 0,
        } => Ok(Block::ReallyBlock(BlockData {
            start: *logtime,
            activity: real_activity,
            tags,
            distribute: distribute.unwrap(),
        })),
        AddOpt {
            really: true,
            activity: _,
            timestamp: _,
            tags: _,
            ago: _,
        } => Err(TTError::new(TTErrorKind::UsageError(
            "really either needs an activity or a manual timestamp (which includes using --before)",
        ))),
        AddOpt {
            really: false,
            activity: Some(activity),
            timestamp: Some(real_timestamp),
            tags,
            ago: before,
        } => Ok(Block::NormalBlock(BlockData {
            start: real_timestamp - Duration::minutes(before),
            activity,
            tags,
            distribute: distribute.unwrap(),
        })),
        AddOpt {
            really: false,
            activity: Some(activity),
            timestamp: None,
            tags,
            ago: before,
        } => Ok(Block::NormalBlock(BlockData {
            start: *logtime - Duration::minutes(before),
            activity,
            tags,
            distribute: distribute.unwrap(),
        })),
        AddOpt {
            really: false,
            activity: None,
            timestamp: _,
            tags: _,
            ago: _,
        } => Err(TTError::new(TTErrorKind::UsageError(
            "please provide an activity.",
        ))),
    }
}

fn is_shortname(s: &std::string::String) -> bool {
    s.starts_with("=")
}

fn shortname_from_tag(s: &String) -> Option<String> {
    if is_shortname(s) {
        Some(s[1..].to_string())
    } else {
        None
    }
}

pub type ActivityMap = HashMap<String, (String, Vec<String>)>;

pub fn read_activities<R: BufRead>(configfile: R) -> Result<ActivityMap, TTError> {
    let mut activity_map: ActivityMap = HashMap::new();
    for line in configfile.lines() {
        let some_line = line?;
        let mut words = some_line.split_whitespace();
        let shortname = words.next();
        let activity = words.next();
        let tags: Vec<String> = words.map(|s| s.to_string()).collect();
        match (shortname, activity) {
            (None, _) => {
                return Err(TTError::new(TTErrorKind::ActivityConfigError(
                    "Config activity lines wrong, correct format is <left> <right> or <activity>",
                )))
            }
            (Some(shortname), None) => {
                activity_map.insert(shortname.to_string(), (shortname.to_string(), vec![]));
                ()
            }
            (Some(shortname), Some(activity)) => {
                activity_map.insert(shortname.to_string(), (activity.to_string(), tags));
                let shortname_for_backlink = if activity_map.contains_key(activity) {
                    vec![]
                } else {
                    vec![shortname.to_string()]
                };
                activity_map.insert(
                    activity.to_string(),
                    (activity.to_string(), shortname_for_backlink),
                );
                ()
            }
        }
    }
    Ok(activity_map)
}

// validates the activity from user input in a block and resolves shortnames etc.
// - recognizes (and removes) a leading '+' as request to add this even when not listed in activitiesfile
// - applies the default prefix from configuration
// - looks up shortnames in the activity_map and applies it
// - returns an error if activity is not in the activitiesfile (and activity started not with a '+')
// - creates new entry in activitymap if a tag provides a shortname (starting with '=')
// - writes a bit of information to stdout if applicable
// The function mutates the data in the passed block in-place
pub fn validate_activity<'a, R: BufRead, W: Write>(
    block: &'a mut Block,
    activity_map: &ActivityMap,
    activitiesfile: &impl FileProxy<R, W>,
) -> Result<(), TTError> {
    if let Block::ReallyBlock(ref mut data) | Block::NormalBlock(ref mut data) = block {
        let is_force_add = data.activity.starts_with("+");
        // let activity = if is_force_add  { &data.activity[1..] } else { &data.activity };
        if is_force_add {
            data.activity = utils::resolve_prefix_for_number(data.activity[1..].as_ref());
            data.distribute = is_distributable(data.activity.as_ref());

            // now check whether we want to add a new shortcut to the activitiesfile
            if !data.distribute
                && !is_break(data.activity.as_ref())
                && !is_start(data.activity.as_ref())
            {
                if let Some(shortname) = data.tags.iter().find_map(shortname_from_tag) {
                    // there is a tag in form =..., so user wants to define a shortcut
                    let is_already_defined =
                        if let Some((found, _found_tags)) = activity_map.get(&shortname) {
                            *found == data.activity
                        } else {
                            false
                        };
                    if !is_already_defined {
                        let other_tags = data.tags.iter().filter(|t| !is_shortname(t)).join(" ");
                        let sep = if other_tags.is_empty() { "" } else { " " };
                        let line = format!("{} {}{}{}", shortname, data.activity, sep, other_tags);
                        writeln!(activitiesfile.writer()?, "{}", line)?;
                        println!("Added to activitiesfile: {}", line);
                    }
                }
            }
        } else if !data.distribute && !is_break(data.activity.as_ref()) && !is_start(&data.activity)
        {
            let activity: String = utils::resolve_prefix_for_number(data.activity.as_ref());

            // Is this a shortcut in the activitiesfile? then use the data from the activitiesfile.
            if let Some((found_activity, found_tags)) = activity_map.get(&activity) {
                data.activity = found_activity.to_string();
                if activity != *found_activity {
                    data.tags.insert(0, format!("={}", activity))
                }
                data.tags.extend_from_slice(found_tags.as_slice());
            } else {
                // since for all shortnames we also add the activity as key into the activity_map,
                // not finding the activity in the map means it is not known at all.
                return Err(TTError::new(TTErrorKind::UsageError(
                    "activity not known, you can add it using the prefix '+'",
                ))
                .context(format!("validating activity {:?}", activity).to_string()));
            }
            data.distribute = is_distributable(data.activity.as_ref());
        }
    }
    Ok(())
}

// handle subcommand "add"
// - validates the activity if activity_map is not None
// - writes the block to the log file
// - if the activity is not already in the actitiviesfile and a tag with starting with = is provided,
//   add the activity to the activitiesfile
// - writes to stdout what it has added
pub fn add<R: BufRead, W: Write>(
    mut block: Block,
    activity_map: Option<&ActivityMap>,
    activitiesfile: &impl FileProxy<R, W>,
    logfile: &impl FileProxy<R, W>,
    timestamp: &NaiveTime,
    now: &DateTime<Local>,
) -> Result<(), TTError> {
    if let Some(activity_map) = activity_map {
        validate_activity(&mut block, activity_map, activitiesfile)?;
    }
    let msg = block.to_string(timestamp);
    println!("{}", msg);
    let mut writer = logfile.writer()?;
    writeln!(writer, "{}", msg)?;
    let collected = collect_blocks(logfile.reader()?.lines(), Some(&now.time()))?;
    report(&collected, &SummaryFormat::Short, &None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::prelude::*;

    use super::utils::FakeFile;
    use super::*;

    #[test]
    fn write_log_normal() {
        let activitiesfile = FakeFile::new(PathBuf::from("configfile"));
        let logfile = FakeFile::new(PathBuf::from("logfile"));
        let opt = AddOpt {
            really: false,
            timestamp: None,
            activity: Some("+test-log".to_string()),
            tags: vec!["=some-tag".to_string()],
            ago: 120,
        };
        let result = run(
            opt,
            &Local.ymd(2020, 1, 1).and_hms(8, 0, 0),
            &logfile,
            &activitiesfile,
        );
        assert!(result.is_ok());
        assert_eq!(logfile.close().unwrap(), "08:00 06:00 test-log =some-tag\n");
        assert_eq!(activitiesfile.close().unwrap(), "some-tag test-log\n");
    }

    #[test]
    fn write_log_unknown_activity() {
        let activitiesfile = FakeFile::new(PathBuf::from("configfile"));
        let logfile = FakeFile::new(PathBuf::from("logfile"));
        let opt = AddOpt {
            really: false,
            timestamp: None,
            activity: Some("test-log".to_string()),
            tags: Vec::new(),
            ago: 0,
        };
        let result = run(
            opt,
            &Local.ymd(2020, 1, 1).and_hms(8, 0, 0),
            &logfile,
            &activitiesfile,
        );
        match result {
            Err(TTError {
                kind: TTErrorKind::UsageError(_),
                context: _,
            }) => (),
            _ => panic!("Expected a LogError::UsageError"),
        }
        assert!(logfile.close().unwrap().is_empty());
        assert!(activitiesfile.close().unwrap().is_empty());
    }

    #[test]
    fn write_log_known_with_tags() {
        let activitiesfile =
            FakeFile::with_content(PathBuf::from("configfile"), b"some-tag test-log\n");
        let logfile = FakeFile::new(PathBuf::from("logfile"));
        let opt = AddOpt {
            really: false,
            timestamp: None,
            activity: Some("some-tag".to_string()),
            tags: vec!["bla".to_string(), "blo".to_string()],
            ago: 0,
        };
        let result = run(
            opt,
            &Local.ymd(2020, 1, 1).and_hms(8, 0, 0),
            &logfile,
            &activitiesfile,
        );
        assert!(result.is_ok());
        assert_eq!(
            logfile.close().unwrap(),
            "08:00 test-log =some-tag bla blo\n"
        );
        assert_eq!(activitiesfile.close().unwrap(), "some-tag test-log\n");
    }

    #[test]
    fn write_log_known_really() {
        let activitiesfile =
            FakeFile::with_content(PathBuf::from("configfile"), b"some-tag test-log\n");
        let logfile = FakeFile::with_content(PathBuf::from("logfile"), b"06:00 first\n");
        let opt = AddOpt {
            really: true,
            timestamp: None,
            activity: Some("some-tag".to_string()),
            tags: vec![],
            ago: 0,
        };
        let result = run(
            opt,
            &Local.ymd(2020, 1, 1).and_hms(8, 0, 0),
            &logfile,
            &activitiesfile,
        );
        assert!(result.is_ok());
        assert_eq!(
            logfile.close().unwrap(),
            "06:00 first\n08:00 really test-log =some-tag\n"
        );
        assert_eq!(activitiesfile.close().unwrap(), "some-tag test-log\n");
    }

    #[test]
    fn write_log_known_with_plus() {
        let activitiesfile =
            FakeFile::with_content(PathBuf::from("configfile"), b"some-tag test-log\n");
        let logfile = FakeFile::new(PathBuf::from("logfile"));
        let opt = AddOpt {
            really: false,
            timestamp: None,
            activity: Some("+test-log".to_string()),
            tags: vec![
                "=some-tag".to_string(),
                "bla".to_string(),
                "blo".to_string(),
            ],
            ago: 0,
        };
        let result = run(
            opt,
            &Local.ymd(2020, 1, 1).and_hms(8, 0, 0),
            &logfile,
            &activitiesfile,
        );
        assert!(result.is_ok());
        assert_eq!(
            logfile.close().unwrap(),
            "08:00 test-log =some-tag bla blo\n"
        );
        assert_eq!(activitiesfile.close().unwrap(), "some-tag test-log\n");
    }
}
