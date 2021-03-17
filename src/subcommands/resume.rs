use crate::error::TTError;
use crate::log_parser::{self, is_distributable, Block, BlockData};
use crate::subcommands;
use crate::utils;
use crate::utils::FileProxy;
use chrono::{DateTime, Local, NaiveTime};
use std::io::{BufRead, Write};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct ResumeOpt {
    #[structopt(short, long)]
    // correct activity or timestamp for previous log entry
    pub really: bool,

    #[structopt(short, long, parse(try_from_str = utils::parse_time))]
    // use this time instead of the current time
    pub timestamp: Option<NaiveTime>,

    #[structopt(name = "n", default_value = "0")]
    // which item on the resume stack - `0` being the most recent
    pub n: usize,
}

pub(crate) fn run<'a, R: BufRead, W: Write, F: FileProxy<R, W>>(
    resume_opt: ResumeOpt,
    now: &DateTime<Local>,
    default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    resume(
        activitiesfile,
        default_logfile,
        now,
        resume_opt.timestamp,
        resume_opt.n,
        resume_opt.really,
    )?;
    Ok(0)
}
pub fn find_resume_activities<R: BufRead>(logfile: R) -> Vec<(String, Vec<String>, usize)> {
    let blocks: Vec<BlockData> = logfile
        .lines()
        .map(log_parser::Block::from_line)
        .filter_map(|b| match b {
            Ok(Block::NormalBlock(data)) | Ok(Block::ReallyBlock(data)) => Some(Ok(data)),
            Ok(Block::CommentBlock(())) => None,
            Ok(Block::TimeCorrection(_)) => None,
            Err(err) => Some(Err(err)),
        })
        .collect::<Result<Vec<_>, TTError>>()
        .unwrap();
    let size = blocks.len();
    let mut result: Vec<_> = Vec::new();
    if size <= 1 {
        return result;
    }
    let mut pos = size - 1;

    let mut current_activity: &str = blocks[pos].activity.as_str();
    loop {
        let block = &blocks[pos];
        // println!("Now at {}: {:?}", pos, block);
        if block.activity.as_str() != current_activity && !log_parser::is_break(&block.activity) {
            result.push((block.activity.clone(), block.tags.clone(), size - pos));
            current_activity = block.activity.as_str();
        }
        let offset: usize = match block.tags.iter().find(|s| s.starts_with("resume:")) {
            None => 1,
            Some(s) => s[7..].parse::<usize>().unwrap_or(1),
        };
        if pos < offset {
            break;
        }
        pos -= offset;
    }
    result
}

pub fn resume<R: BufRead, W: Write, F: FileProxy<R, W>>(
    activitiesfile: &F,
    logfile: &F,
    now: &DateTime<Local>,
    timestamp: Option<NaiveTime>,
    n: usize,
    really: bool,
) -> Result<(), TTError> {
    let resume_stack = find_resume_activities(logfile.reader()?);
    if resume_stack.is_empty() {
        println!("Nothing to resume.");
        return Ok(());
    }
    let (activity, original_tags, offset) = resume_stack
        .get(n)
        .expect("Wrong number. Aborting.")
        .clone();
    let mut tags: Vec<String> = original_tags
        .into_iter()
        .filter(|s| !s.starts_with("resume:"))
        .collect();
    tags.push(format!("resume:{}", offset).to_string());

    let data = BlockData {
        start: timestamp.unwrap_or_else(|| now.naive_local().time()),
        activity: format!("+{}", activity),
        tags,
        distribute: is_distributable(activity.as_ref()),
    };
    let activity_map = subcommands::add::read_activities(activitiesfile.reader()?)
        .expect("Cannot read config file");
    let block = match really {
        true => Block::ReallyBlock,
        false => Block::NormalBlock,
    }(data);
    subcommands::add::add(
        block,
        Some(&activity_map),
        activitiesfile,
        logfile,
        &timestamp.unwrap_or_else(|| now.time()),
        now,
    )
}
