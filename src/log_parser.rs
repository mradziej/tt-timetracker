use std::io::{BufRead, Write};
use std::result::Result;
use std::str;
use std::vec::Vec;

use chrono::NaiveTime;

use crate::error::{TTError, TTErrorKind};
use crate::subcommands::add::read_activities;
use crate::subcommands::watch_i3;
use crate::utils::FileProxy;

use super::utils;

#[derive(Debug, Eq, PartialEq)]
pub struct BlockData {
    pub start: NaiveTime,
    pub activity: String,
    pub tags: Vec<String>,
    pub distribute: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Block {
    NormalBlock(BlockData),
    ReallyBlock(BlockData),
    TimeCorrection(NaiveTime),
    CommentBlock(()),
}

/// returns whether the time for this activity will be distributed to the other activities of the day
/// ```
/// use timetracker::log_parser;
/// assert_eq!(log_parser::is_distributable("normal"), false);
/// assert_eq!(log_parser::is_distributable("_distrib"), true);
/// ```
pub fn is_distributable(activity: &str) -> bool {
    activity.starts_with("_")
}

/// returns whether this activity is a break or end of day
/// ```
/// use timetracker::log_parser;
/// assert_eq!(log_parser::is_break("normal"), false);
/// assert_eq!(log_parser::is_break("break"), true);
/// ```
pub fn is_break(activity: &str) -> bool {
    activity == "break" || activity == "end"
}

/// returns whether this activity is the start of something yet unknown
/// ```
/// use timetracker::log_parser;
/// assert_eq!(log_parser::is_start("start"), true);
/// assert_eq!(log_parser::is_start("_start"), true);
/// assert_eq!(log_parser::is_start("break"), false);
/// ```
pub fn is_start(activity: &str) -> bool {
    activity == "start" || activity == "_start"
}

impl Block {
    /// parses a log line into a Block
    ///
    /// It accepts (and passes on) io errors for easy use with line iterators.
    /// Since it uses the passed line string to error messages, it needs to own the line.
    ///
    /// ```
    /// use timetracker::log_parser::{Block, BlockData};
    /// use chrono::NaiveTime;
    /// use std::vec;
    /// assert_eq!(Block::from_line(Ok("10:13 started".to_string())).unwrap(),
    ///     Block::NormalBlock(BlockData{
    ///         start: NaiveTime::from_hms(10,13,0),
    ///         activity: "started".to_string(),
    ///         tags: vec![],
    ///         distribute: false,
    /// }));
    /// ```
    ///
    pub fn from_line(line: Result<String, std::io::Error>) -> Result<Block, TTError> {
        let line = line?;
        let mut words = line.split_whitespace();
        let mut next_word = match words.next() {
            None => return Ok(Block::CommentBlock(())),
            Some(comment) if comment.starts_with("#") => return Ok(Block::CommentBlock(())),
            Some(word) => word,
        };
        let mut start = utils::parse_time(next_word).map_err(|_| {
            TTError::new(TTErrorKind::ParseError(
                "cannot parse start time",
                line.to_string(),
            ))
        })?;
        next_word = words.next().ok_or_else(|| {
            TTError::new(TTErrorKind::ParseError(
                "Line does not contain at least 2 words",
                line.to_string(),
            ))
        })?;
        if next_word.starts_with("#") {
            return Ok(Block::CommentBlock(()));
        };
        let is_really_block = next_word == "really";
        if is_really_block {
            next_word = words.next().ok_or_else(|| {
                TTError::new(TTErrorKind::ParseError(
                    "really block does not contain an activity",
                    line.to_string(),
                ))
            })?;
        }
        let manual_time = utils::parse_time(next_word);
        if let Ok(manual_time) = manual_time {
            if is_really_block {
                return if words.next().is_some() {
                    Err(TTError::new(TTErrorKind::ParseError("time correction cannot have further data, i.e. use only <time> really <time>", line.to_string())))
                } else {
                    Ok(Block::TimeCorrection(manual_time))
                };
            }
            start = manual_time;
            next_word = words.next().ok_or_else(|| {
                TTError::new(TTErrorKind::ParseError(
                    "really block does not contain an activity",
                    line.to_string(),
                ))
            })?;
        }

        let data = BlockData {
            start,
            activity: utils::resolve_prefix_for_number(&next_word),
            tags: words.map(|s| s.to_string()).collect(),
            distribute: is_distributable(&next_word),
        };
        match is_really_block {
            true => Ok(Block::ReallyBlock(data)),
            false => Ok(Block::NormalBlock(data)),
        }
    }

    pub fn from_data(data: BlockData, really: bool) -> Block {
        if really {
            Block::ReallyBlock(data)
        } else {
            Block::NormalBlock(data)
        }
    }

    /// Turns the block into a log file line (without trailing '\n')
    /// ```
    /// use timetracker::log_parser::{Block, BlockData};
    /// use chrono::NaiveTime;
    /// let block = Block::NormalBlock(BlockData{
    ///         start: NaiveTime::from_hms(10,13,0),
    ///         activity: "started".to_string(),
    ///         tags: vec!["tag-one".to_string(),"tag-two".to_string()],
    ///         distribute: false,
    /// });
    /// let result = block.to_string(&NaiveTime::from_hms(10, 13, 0));
    /// assert_eq!(result, "10:13 started tag-one tag-two");
    /// ```
    pub fn to_string(&self, timestamp: &NaiveTime) -> String {
        let really = match self {
            Block::ReallyBlock(_) => true,
            _ => false,
        };
        let msg = match self {
            Block::NormalBlock(data) | Block::ReallyBlock(data) => {
                let BlockData {
                    start,
                    activity,
                    tags,
                    distribute: _distribute,
                } = data;
                let mut words: Vec<&str> = Vec::with_capacity(5 + tags.len());

                let timestamp_str = utils::format_time(timestamp).to_string();
                let start_str = utils::format_time(&start).to_string();
                words.push(&timestamp_str);
                if really {
                    words.push("really")
                };
                if *start != *timestamp {
                    words.push(&start_str)
                }
                words.push(activity);
                words.extend(tags.iter().map(|t| t as &str));
                words.join(" ")
            }
            Block::TimeCorrection(manual_time) => format!(
                "{timestamp} really {manual_time}",
                timestamp = utils::format_time(timestamp),
                manual_time = utils::format_time(manual_time)
            ),
            Block::CommentBlock(()) => "".to_string(),
        };
        msg.to_string()
    }
}

#[derive(Debug, PartialEq)]
pub struct TTInfo {
    pub activity: String,
    pub shortname: Option<String>,
}

impl TTInfo {
    pub fn from_string<R: BufRead, W: Write>(
        activityfile: &impl FileProxy<R, W>,
        name: &str,
    ) -> TTInfo {
        if is_start(name) || name.is_empty() {
            TTInfo {
                activity: "start".to_string(),
                shortname: None,
            }
        } else {
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
    }

    // as workspace title, we prefer to use the shortname
    pub fn as_workspace_title(&self) -> &str {
        let title = watch_i3::as_str(&self.shortname).unwrap_or_else(|| self.activity.as_str());
        if is_start(title) {
            ""
        } else {
            title
        }
    }

    // as an activity for a block to add to the activity log
    pub fn as_block_activity(&self) -> String {
        if self.shortname.is_none()
            && !is_start(&self.activity)
            && !is_break(&self.activity)
            && !self.activity.starts_with("_")
        {
            format!("+{}", self.activity)
        } else {
            self.activity.to_string()
        }
    }

    // provide the tags for a block if we use self.as_tt_activity as activity
    pub fn tags(&self) -> Vec<String> {
        match self.shortname {
            None => vec![],
            Some(ref s) => vec![format!("={}", s)],
        }
    }
}
