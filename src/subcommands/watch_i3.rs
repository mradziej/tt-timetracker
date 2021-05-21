use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::ops::Sub;
use std::process::Command;
use std::thread::sleep;

use chrono::{self, DateTime, Local, NaiveTime};
use if_chain::if_chain;
use itertools::all;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;

use crate::collector::collect_blocks;
use crate::configfile::TTConfig;
use crate::error::TTError;
use crate::log_parser::{is_break, is_distributable, is_start, Block, BlockData, TTInfo};
use crate::subcommands::add::{add, read_activities, ActivityMap};
use crate::utils::FileProxy;
use std::cmp::min;

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

pub(crate) fn watch_i3<R: BufRead, W: Write>(
    logfile: &impl FileProxy<R, W>,
    activitiesfile: &impl FileProxy<R, W>,
) -> Result<(), TTError> {
    let mut focus_counter: HashMap<String, u16> = HashMap::new();
    let mut prev_tt_activity: Option<TTInfo> = None;
    let config = TTConfig::get();
    let granularity = config.watch_i3.granularity;
    let min_count = (config.watch_i3.timeblock.as_secs() / granularity.as_secs()) as u16;
    let min_count_empty =
        (config.watch_i3.timeblock_empty.as_secs() / granularity.as_secs()) as u16;
    let mut max_count: u16 = 0;
    let mut known_ws: HashSet<i16> = get_workspaces()
        .unwrap_or_default()
        .iter()
        .map(|ws| ws.num)
        .collect();
    let mut prev_focus_name: Option<String> = None;

    loop {
        let now = chrono::Local::now();
        let activity_map: Option<ActivityMap> = activitiesfile
            .reader()
            .ok()
            .and_then(|file| read_activities(file).ok());
        let collected = (|| {
            collect_blocks(logfile.reader().ok()?.lines(), None)
                .ok()
                .flatten()
        })();
        let tt_activity = collected
            .as_ref()
            .map(|coll| TTInfo::from_string(activitiesfile, &coll.final_activity));
        if tt_activity != prev_tt_activity {
            focus_counter.clear();
            max_count = 0;
            prev_tt_activity = tt_activity;
        } else {
            let ws_list = get_workspaces()?;
            known_ws = ws_list
                .iter()
                .map(|ws| ws.num)
                .filter(|num| known_ws.contains(num))
                .collect();

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
            if focus_count > max_count {
                max_count = focus_count;
            }

            if let Some(focus_ws) = focus_ws {
                // new window without title?
                if !known_ws.contains(&focus_ws.num) && focus_ws.activity().is_none() {
                    known_ws.insert(focus_ws.num);
                    if let Some(tt_activity) = &tt_activity {
                        let new_name = tt_activity.as_workspace_title();
                        if !is_start(new_name) && !is_break(new_name) {
                            println!("Workspace {}: {}", focus_ws.num, new_name);
                            set_ws_name(focus_ws, new_name);
                        }
                    }
                } else if focus_count >= max_count && focus_count >= min(min_count, min_count_empty)
                {
                    let focus_activity = TTInfo::from_string(
                        activitiesfile,
                        focus_ws.activity().unwrap_or_default(),
                    );
                    let min_required = if focus_activity.as_workspace_title().is_empty() {
                        min_count_empty
                    } else {
                        min_count
                    };
                    if focus_count >= min_required {
                        focus_counter.clear();
                        max_count = 0;
                        let focus_workspace_title = focus_activity.as_workspace_title();
                        if let Some(tt_activity) = tt_activity {
                            if !is_break(&tt_activity.activity)
                                && tt_activity.as_workspace_title() != focus_workspace_title
                                && prev_focus_name
                                    .as_ref()
                                    .map(|s| s != focus_workspace_title)
                                    .unwrap_or(true)
                            {
                                // workspace title != current activity,  consider to add a tt block
                                prev_focus_name = Some(focus_workspace_title.to_string());
                                let start = now
                                    .sub(
                                        chrono::Duration::from_std(
                                            granularity * focus_count as u32,
                                        )
                                        .map_err(|err| TTError::from(err.to_string()))?,
                                    )
                                    .time()
                                    .max(collected.map_or(NaiveTime::from_hms(0, 0, 0), |coll| {
                                        coll.final_start
                                    }));
                                let really = is_start(&tt_activity.as_block_activity());
                                let data = BlockData {
                                    start: if really { now.time() } else { start },
                                    activity: focus_activity.as_block_activity(),
                                    tags: focus_activity.tags(),
                                    distribute: is_distributable(&focus_activity.activity),
                                };
                                add(
                                    Block::from_data(data, really),
                                    activity_map.as_ref(),
                                    activitiesfile,
                                    logfile,
                                    &now.time(),
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
