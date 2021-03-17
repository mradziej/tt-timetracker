use chrono::{self, DateTime, Local};
use if_chain::if_chain;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use serde_json;
use std::io::{BufRead, Write};

use crate::collector::collect_blocks;
use crate::error::TTError;
use crate::log_parser::{is_break, is_distributable, Block, BlockData};
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

#[derive(Debug, Clone)]
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
        as_str(&self.shortname).unwrap_or(self.activity.as_str())
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
pub(crate) fn run<'a, R: BufRead, W: Write, F: FileProxy<R, W>>(
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
    let min_visible = chrono::Duration::minutes(3);
    let mut prev_activity: Option<ActivityInfo> = None;
    let mut prev_focus: HashMap<String, FocusInfo> = HashMap::new();

    loop {
        let now = chrono::Local::now();
        let ws_list = get_workspaces()?;

        // track changes in focus per output
        let focus_wm = ws_list.iter().find(|ws| ws.focused);
        if let Some(focus_wm) = focus_wm {
            let focus_output = focus_wm.output.as_str();
            if !prev_focus.contains_key(focus_output) {
                prev_focus.insert(
                    focus_output.to_string(),
                    FocusInfo {
                        since: now,
                        num: focus_wm.num,
                    },
                );
            }
        } else {
            prev_focus.clear();
        }

        // track changes in activity
        let maybe_focus_activity = focus_wm.map(|ws| ws.activity()).flatten();
        let maybe_previous_activity_name = prev_activity.as_ref().map(|p| p.activity.as_str());
        let stable_focus_activity = match (maybe_focus_activity, maybe_previous_activity_name) {
            (None, _) => {
                prev_activity = None;
                None
            }
            (Some(focus_activity), None) => {
                prev_activity = Some(ActivityInfo {
                    activity: focus_activity.to_string(),
                    since: now,
                });
                None
            }
            (Some(focus_activity), Some(prev_activity_name)) => {
                if focus_activity != prev_activity_name {
                    prev_activity = Some(ActivityInfo {
                        activity: focus_activity.to_string(),
                        since: now,
                    });
                    None
                } else if now.sub(prev_activity.as_ref().unwrap().since) > min_visible {
                    Some(prev_activity_name)
                } else {
                    None
                }
            }
        };

        // consider to add a tt block
        if let Some(stable_focus_activity) = stable_focus_activity {
            let activity_map: Option<ActivityMap> = activitiesfile
                .reader()
                .ok()
                .and_then(|file| read_activities(file).ok());
            let tt_activity = get_current_activity(logfile, activitiesfile);
            let focus_since = prev_activity.as_ref().unwrap().since;
            if tt_activity.as_ref().map_or(true, |tt| {
                !is_break(&tt.activity) && tt.as_workspace_title() != stable_focus_activity
            }) {
                let focus_activity = TTInfo::from_string(activitiesfile, stable_focus_activity); // TODO: get rid, we only need to know if activity_map has an entry
                let data = BlockData {
                    start: focus_since.time(),
                    activity: focus_activity.activity.to_string(),
                    tags: focus_activity.tags(),
                    distribute: is_distributable(stable_focus_activity),
                };
                add(
                    Block::from_data(
                        data,
                        tt_activity
                            .as_ref()
                            .map_or(false, |ref tt| tt.activity == "_start"),
                    ),
                    activity_map.as_ref(),
                    activitiesfile,
                    logfile,
                    &focus_since.time(),
                    &now,
                )?; // TODO really, error
            }
        } else {
            if_chain! {
                // consider to name the focus workspace
                if let Some(focus_wm) = focus_wm;
                if focus_wm.num != 1 && focus_wm.num != 9 && focus_wm.activity().is_none();
                if now.sub(prev_focus.get(focus_wm.output.as_str()).unwrap().since) > min_visible;
                if let Some(current_activity) = get_current_activity(logfile, activitiesfile).as_ref();
                if ! is_break(&current_activity.activity) && current_activity.activity != "_start";
                if all(&ws_list, |wm| wm.output != focus_wm.output || wm.num == focus_wm.num || wm.activity() != Some(current_activity.as_workspace_title()));
                then {
                    println!("Workspace {}: {}", focus_wm.num, current_activity.as_workspace_title());
                    set_ws_name(focus_wm, current_activity.as_workspace_title());
                }
            }
        }
        sleep(core::time::Duration::from_secs(10));
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
