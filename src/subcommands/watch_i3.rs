use chrono::{self, DateTime, Local};
use if_chain::if_chain;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::io::{BufRead, Write};

use crate::collector::collect_blocks;
use crate::configfile::TTConfig;
use crate::error::TTError;
use crate::log_parser::{is_break, is_distributable, is_start, Block, BlockData};
use crate::subcommands::add::{add, read_activities, ActivityMap};
use crate::utils::FileProxy;
use itertools::all;
use std::collections::HashMap;
use std::ops::Sub;
use std::process::Command;
use std::thread::sleep;

#[derive(Copy, Clone, Deserialize, Debug)]
struct I3Rectangle {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

#[derive(Clone, Deserialize, Debug)]
struct I3Workspace {
    num: i16,
    name: String,
    visible: bool,
    focused: bool,
    rect: I3Rectangle,
    output: String,
    urgent: bool,
}

#[derive(Debug, Clone)]
struct ActivityInfo {
    since: DateTime<Local>,
    activity: String,
}

#[derive(Debug, Clone)]
struct FocusInfo {
    since: DateTime<Local>,
    num: i16,
}

#[derive(Debug, Clone, PartialEq)]
struct TTInfo {
    activity: String,
    shortname: Option<String>,
}

impl TTInfo {
    fn from_string<R: BufRead, W: Write>(
        activityfile: &impl FileProxy<R, W>,
        name: &str,
    ) -> TTInfo {
        let activity_map = activityfile
            .reader()
            .ok()
            .and_then(|file| read_activities(file).ok());
        match activity_map.as_ref().and_then(|map| map.get(name)) {
            Some((activity, tags)) if activity == name => TTInfo {
                activity: activity.to_string(),
                shortname: tags.first().map(|s| s.to_string()),
            },
            Some((activity, _tags)) => TTInfo {
                activity: activity.to_string(),
                shortname: Some(name.to_string()),
            },
            None => TTInfo {
                activity: name.to_string(),
                shortname: None,
            },
        }
    }

    // as workspace title, we prefer to use the shortname
    fn as_workspace_title(&self) -> &str {
        as_str(&self.shortname).unwrap_or_else(|| self.activity.as_str())
    }

    // as an activity for a block to add to the activity log
    fn as_block_activity(&self) -> String {
        if self.shortname.is_none() {
            format!("+{}", self.activity)
        } else {
            self.activity.to_string()
        }
    }

    // provide the tags for a block if we use self.as_tt_activity as activity
    fn tags(&self) -> Vec<String> {
        match self.shortname {
            None => vec![],
            Some(ref s) => vec![format!("={}", s)],
        }
    }
}

lazy_static! {
    static ref WSACTIVITY_RE: Regex = Regex::new(r"^(?:\d*:)?\s*([[:alpha:]_].*)$").unwrap();
}

impl I3Workspace {
    fn activity(self: &I3Workspace) -> Option<&str> {
        Some(WSACTIVITY_RE.captures(&self.name)?.get(1)?.as_str())
    }
}

pub(crate) fn as_str(opt: &Option<String>) -> Option<&str> {
    Some(opt.as_ref()?.as_str())
}

// interface for the runner, adds what is in the opt to the logfile
// public interface is the function add()
pub(crate) fn run<R: BufRead, W: Write, F: FileProxy<R, W>>(
    _now: &DateTime<Local>,
    default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    watch_i3(default_logfile, activitiesfile)?;
    Ok(0)
}

fn get_workspaces() -> Result<Vec<I3Workspace>, TTError> {
    // run i3-msg -t get_workspaces, parse json output
    let output = Command::new("i3-msg")
        .arg("-t")
        .arg("get_workspaces")
        .output()?;
    Ok(serde_json::from_slice(output.stdout.as_slice())?)
}

fn get_current_activity<R: BufRead, W: Write>(
    logfile: &impl FileProxy<R, W>,
    activityfile: &impl FileProxy<R, W>,
) -> Option<TTInfo> {
    let activity = collect_blocks(logfile.reader().ok()?.lines(), None)
        .ok()??
        .final_activity;
    Some(TTInfo::from_string(activityfile, &activity))
}

pub(crate) fn watch_i3<R: BufRead, W: Write>(
    logfile: &impl FileProxy<R, W>,
    activitiesfile: &impl FileProxy<R, W>,
) -> Result<(), TTError> {
    let mut focus_counter: HashMap<String, u16> = HashMap::new();
    let mut prev_tt_activity: Option<TTInfo> = None;
    let config = TTConfig::get();
    let granularity = config.watch_i3.granularity;
    let min_count = (config.watch_i3.timeblock.as_secs() / granularity.as_secs()) as u16;

    loop {
        let now = chrono::Local::now();
        let activity_map: Option<ActivityMap> = activitiesfile
            .reader()
            .ok()
            .and_then(|file| read_activities(file).ok());
        let tt_activity = get_current_activity(logfile, activitiesfile);

        if tt_activity != prev_tt_activity {
            focus_counter.clear();
            prev_tt_activity = tt_activity;
        } else {
            let ws_list = get_workspaces()?;

            // track changes in focus per output
            let focus_ws = ws_list.iter().find(|ws| ws.focused);
            let focus_count = match focus_ws {
                Some(focus_wm) => {
                    let focus_output = focus_wm.output.as_str();
                    match focus_counter.get_mut(focus_output) {
                        None => {
                            focus_counter.insert(focus_output.to_string(), 1);
                            1
                        }
                        Some(v) => {
                            let new_count = *v + 1;
                            *v = new_count;
                            new_count
                        }
                    }
                }
                None => 0,
            };

            // do we have a focus time long enough?
            if focus_count >= min_count {
                focus_counter.clear();
                if tt_activity
                    .as_ref()
                    .map(|activity| !is_break(&activity.activity) && !is_start(&activity.activity))
                    .unwrap_or(true)
                {
                    match focus_ws
                        .and_then(|ws| ws.activity())
                        .map(|name| TTInfo::from_string(activitiesfile, name))
                    {
                        None => if_chain! {
                                // no workspace title, consider to rename the workspace
                            if let Some(tt_activity) = &tt_activity;
                            if let Some(focus_ws) = focus_ws;
                            if focus_ws.activity().is_none();
                            if all(&ws_list, |ws| ws.output != focus_ws.output || ws.num == focus_ws.num || ws.activity() != Some(tt_activity.as_workspace_title()));
                            then {
                                println!("Workspace {}: {}", focus_ws.num, tt_activity.as_workspace_title());
                                set_ws_name(focus_ws, tt_activity.as_workspace_title());
                            }
                        },
                        Some(focus_activity) => {
                            if tt_activity
                                .as_ref()
                                .map(|tt| {
                                    tt.as_workspace_title() != focus_activity.as_workspace_title()
                                })
                                .unwrap_or(true)
                            {
                                // workspace title != current activity,  consider to add a tt block
                                let start = now
                                    .sub(
                                        chrono::Duration::from_std(
                                            granularity * focus_count as u32,
                                        )
                                        .map_err(|err| TTError::from(err.to_string()))?,
                                    )
                                    .time();
                                let data = BlockData {
                                    start,
                                    activity: focus_activity.as_block_activity(),
                                    tags: focus_activity.tags(),
                                    distribute: is_distributable(&focus_activity.activity),
                                };
                                add(
                                    Block::from_data(data, false),
                                    activity_map.as_ref(),
                                    activitiesfile,
                                    logfile,
                                    &start,
                                    &now,
                                )?;
                            }
                        }
                    }
                }
            }
        }
        sleep(granularity);
    }
}

fn set_ws_name(ws: &I3Workspace, activity: &str) {
    let _output = Command::new("i3-msg")
        .arg(format!(
            "rename workspace \"{}\" to \"{}: {}",
            ws.name, ws.num, activity
        ))
        .output();
}
