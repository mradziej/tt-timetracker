use chrono::NaiveTime;
use chrono::{DateTime, Duration, Local};
use chrono::{Datelike, NaiveDate};
use core::str::FromStr;
use itertools::Itertools;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};

use std::fmt;
use std::result::Result;
use std::str;
use structopt::StructOpt;

// use crate::SummaryFormat::{Short, Long};

use crate::collector::{collect_blocks, ActivityHashMap, CollectResult, Summary};
use crate::error::TTError;
use crate::get_logfile_name;
use crate::log_parser;
use crate::subcommands::add::{read_activities, ActivityMap};
use crate::utils;
use crate::utils::{format_duration, format_time, FileProxy};
use std::collections::hash_map::RandomState;

#[derive(StructOpt, Debug)]
pub(crate) struct ReportOpt {
    #[structopt(short, long)]
    // create a summary in this format
    pub format: Option<SummaryFormat>,

    #[structopt(long)]
    // create report for this day (default is today), only for reporting
    pub date: Option<NaiveDate>,

    #[structopt(short, long)]
    // create report for yesterday
    pub yesterday: bool,

    #[structopt(short, long)]
    // create report for the calendar week
    pub week: bool,

    #[structopt(short, long, parse(try_from_str = utils::parse_duration))]
    // tasks with less that duration are distributed
    pub cutoff: Option<Duration>,

    #[structopt(short, long)]
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SummaryFormat {
    Status,   // status for i3bar or so
    Short,    // summary that you get also when adding a new entry
    Long,     // full summary
    Tickets,  // show the ticket ids
    Table,    // make a table, nice for week reporting
    Activity, // current activity, uses shortname if present
    Ticket,   // current activity, does NOT use shortname
    Worktime, // worktime today in minutes
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSummaryFormatError {
    _priv: (),
}

impl FromStr for SummaryFormat {
    type Err = ParseSummaryFormatError;
    fn from_str(s: &str) -> Result<SummaryFormat, ParseSummaryFormatError> {
        match s {
            "short" => Ok(SummaryFormat::Short),
            "long" => Ok(SummaryFormat::Long),
            "status" => Ok(SummaryFormat::Status),
            "tickets" => Ok(SummaryFormat::Tickets),
            "table" => Ok(SummaryFormat::Table),
            "activity" => Ok(SummaryFormat::Activity),
            "ticket" => Ok(SummaryFormat::Ticket),
            "worktime" => Ok(SummaryFormat::Worktime),
            _ => Err(ParseSummaryFormatError { _priv: () }),
        }
    }
}

impl std::fmt::Display for ParseSummaryFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "provided string was not `short` or `long` or `status` or `table` or `tickets`".fmt(f)
    }
}

// interface for the runner. Public interface is report()
pub(crate) fn run<'a, R: BufRead, W: Write, F: FileProxy<R, W>>(
    report_opt: ReportOpt,
    now: &DateTime<Local>,
    _default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    let format = match report_opt.format {
        Some(ref f) => f,
        None if report_opt.week => &SummaryFormat::Table,
        _ => &SummaryFormat::Long,
    };
    report_opt.format.clone().unwrap_or(SummaryFormat::Long);
    let mut summaries = Vec::new();
    for (date, add_ending_at) in report_dates(&report_opt, *now) {
        let logfile_reader = F::new(get_logfile_name(&date)).reader();
        let collected = collect_blocks(logfile_reader?.lines(), add_ending_at.as_ref())?;
        if format != &SummaryFormat::Table {
            if report_opt.week {
                println!("{}:\n", date);
            }
            report(&collected, &format, &report_opt.cutoff);
        }
        if let Some(collected) = collected {
            summaries.push((date, collected.summary));
        }
    }
    if format == &SummaryFormat::Table {
        let activity_map = read_activities(activitiesfile.reader()?)?;
        report_table(&summaries, report_opt.cutoff, report_opt.all, &activity_map);
    }
    Ok(0)
}

fn report_dates(
    opt: &ReportOpt,
    now: DateTime<Local>,
) -> Box<dyn Iterator<Item = (NaiveDate, Option<NaiveTime>)>> {
    let date = now.naive_local().date();
    let mut base_date = date;
    if let Some(explicit_date) = opt.date {
        base_date = explicit_date;
    }
    if opt.yesterday {
        base_date = base_date.pred();
    }
    let offset = if opt.week {
        base_date.weekday().num_days_from_monday() as i64
    } else {
        0
    };
    let first = base_date - Duration::days(offset);
    Box::new(
        (0..offset + 1)
            .map(move |i| first + Duration::days(i))
            .map(move |d| {
                (
                    d,
                    match d == date {
                        true => Some(now.naive_local().time()),
                        false => None,
                    },
                )
            }),
    )
}

fn report_sort_key(
    kv: &(&String, &(Duration, BTreeMap<String, Duration>)),
) -> (bool, bool, Duration) {
    (
        log_parser::is_break(&kv.0),
        log_parser::is_distributable(&kv.0),
        -(kv.1).0,
    )
}

pub(crate) fn short_report(summary: &Summary) -> String {
    format!(
        "start: {}, end: {}, breaks: {}, work time: {}, distribute: {}",
        utils::format_time(&summary.start),
        utils::format_time(&summary.end),
        utils::format_duration(&summary.breaks),
        utils::format_duration(&summary.work_time),
        utils::format_duration(&summary.distribute),
    )
    .to_string()
}

fn handle_cutoff_and_distribute(
    summary: &Summary,
    cutoff: Option<Duration>,
    all: bool,
) -> HashMap<&String, (Duration, &BTreeMap<String, Duration>)> {
    let cutoff_total_d = cutoff_sum(&summary.activities, &cutoff);
    let cutoff_total = cutoff_total_d.num_seconds() as f64;
    let cutoff = cutoff.unwrap_or_else(Duration::zero);
    let distribute = if all {
        cutoff_total
    } else {
        cutoff_total + summary.distribute.num_seconds() as f64
    };
    summary
        .activities
        .iter()
        .filter(|(name, (duration, duration_map))| {
            !log_parser::is_break(name)
                && (all || !log_parser::is_distributable(name))
                && *duration >= cutoff
        })
        .map(|(name, (duration, duration_map))| {
            let share = Duration::seconds(
                (distribute
                    * (duration.num_seconds() as f64
                        / (summary.work_time.num_seconds() as f64
                            - summary.distribute.num_seconds() as f64
                            - cutoff_total))) as i64,
            );
            (name, ((*duration + share), duration_map))
        })
        .collect()
}

// return the long status report as an iterator of lines; it consumes summary.
pub(crate) fn long_report_lines(
    summary: &Summary,
    cutoff: &Option<Duration>,
) -> Vec<(String, String)> {
    let cutoff_total_d = cutoff_sum(&summary.activities, cutoff);
    let cutoff_total = cutoff_total_d.num_seconds() as f64;
    let cutoff = cutoff.unwrap_or_else(Duration::zero);
    let mut activities: Vec<(&String, &(Duration, BTreeMap<String, Duration>))> =
        summary.activities.iter().collect();
    activities.sort_unstable_by_key(report_sort_key); // |kv: &(String, Duration)| (kv.0 == "break", kv.0.starts_with("_"), -kv.1));
    activities
        .iter()
        .map(|(name, (duration, duration_map))| {
            (
                if log_parser::is_distributable(name)
                    || log_parser::is_break(name)
                    || duration < &cutoff
                {
                    format!("- {:16}({})", name, utils::format_duration(&duration)).to_string()
                } else {
                    let share = Duration::seconds(
                        ((summary.distribute.num_seconds() as f64 + cutoff_total)
                            * (duration.num_seconds() as f64
                                / (summary.work_time.num_seconds() as f64
                                    - summary.distribute.num_seconds() as f64
                                    - cutoff_total))) as i64,
                    );
                    let tag_report = duration_map
                        .iter()
                        .filter(|(t, _duration)| !t.starts_with("resume:"))
                        .map(|(tag, tag_duration)| {
                            format!(
                                "{}{}",
                                tag,
                                if tag.starts_with("=") || *tag_duration == *duration {
                                    "".to_string()
                                } else {
                                    format!("({})", utils::format_duration(tag_duration))
                                }
                            )
                        })
                        .join(", ");
                    format!(
                        "- {:16} {} + {} = {}  {}",
                        name,
                        utils::format_duration(&duration),
                        utils::format_duration(&share),
                        utils::format_duration(&(*duration + share)),
                        tag_report
                    )
                    .to_string()
                },
                name.to_string(),
            )
        })
        .collect()
}

// returns the sum of the activities with a duration below the cutoff time
fn cutoff_sum(activities: &ActivityHashMap, cutoff: &Option<Duration>) -> Duration {
    match cutoff {
        None => Duration::zero(),
        Some(cutoff) => activities
            .iter()
            .filter_map(|(activity, (duration, _tags))| {
                if !log_parser::is_break(activity)
                    && !log_parser::is_distributable(activity)
                    && duration < cutoff
                {
                    Some(duration)
                } else {
                    None
                }
            })
            .fold(Duration::zero(), |a, b| a + *b),
    }
}

// status: <last-activity> since <time> (<duration>) total (work-time) distrib (distributable)
pub fn report(
    collect_result: &Option<CollectResult>,
    format: &SummaryFormat,
    cutoff: &Option<Duration>,
) {
    match collect_result {
        None => {
            println!("No activities found.");
            ()
        }
        Some(CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        }) => match format {
            SummaryFormat::Status => {
                println!(
                    "{}{} since {} ({}) wt: {} dt: {}",
                    final_activity,
                    if let Some(final_shortname) = final_shortname {
                        format!(" {}", final_shortname)
                    } else {
                        "".to_string()
                    },
                    utils::format_time(&final_start),
                    utils::format_duration(
                        &summary
                            .activities
                            .get(final_activity)
                            .map_or_else(Duration::zero, |(duration, _map)| *duration)
                    ),
                    utils::format_duration(&summary.work_time),
                    utils::format_duration(&summary.distribute),
                );
                ()
            }
            SummaryFormat::Tickets => {
                for (activity, _tag_durations) in summary.activities.iter().sorted() {
                    println!("{}", activity);
                }
                ()
            }
            SummaryFormat::Long | SummaryFormat::Short => {
                println!("{}", short_report(&summary));
                if let SummaryFormat::Long = format {
                    for (line, _activity) in long_report_lines(&summary, cutoff) {
                        println!("{}", line);
                    }
                }
                ()
            }
            SummaryFormat::Activity => {
                println!("{}", final_shortname.as_ref().unwrap_or(final_activity));
            }
            SummaryFormat::Ticket => {
                println!("{}", final_activity);
            }
            SummaryFormat::Worktime => {
                println!("{}", summary.work_time.num_minutes())
            }
            SummaryFormat::Table => (), // special case handled in caller
        },
    }
}

struct DailyActivity {
    durations: HashMap<NaiveDate, Duration>,
    total: Duration,
}

impl Default for DailyActivity {
    fn default() -> Self {
        DailyActivity {
            durations: HashMap::default(),
            total: Duration::zero(),
        }
    }
}

fn report_table(
    summaries: &Vec<(NaiveDate, Summary)>,
    cutoff: Option<Duration>,
    all: bool,
    activity_map: &ActivityMap,
) {
    // Table-Format:
    // XXXX Mo Di Mi Do Fr Sa So Sum
    // ----
    // Start
    // End
    // break
    // Worktime
    // ----
    // Ticket-1
    // Ticket.2

    // First part is easy directly with the summaries.
    // After that we need a list of tickets with the sums for each day and totals
    // let max_length = summaries.map(|s| s.)
    let mut activities: HashMap<&str, DailyActivity, RandomState> = HashMap::new(); // &str, DailyActivity>::new();
    for (date, summary) in summaries {
        let filtered_activities = handle_cutoff_and_distribute(&summary, cutoff, all);
        for (name, (duration, ..)) in filtered_activities {
            let mut daily = activities.entry(&name).or_default();
            daily.total = daily.total + duration;
            daily.durations.insert(*date, duration);
        }
    }
    // let normalized_activities = handle_cutoff_and_distribute(summary)
    let activity_names: Vec<&&str> = activities.keys().sorted().collect();
    let activity_length = activity_names.iter().map(|s| s.len()).max().unwrap();
    let shortname_length = activity_names
        .iter()
        .map(|s| {
            activity_map
                .get(**s)
                .map(|(s, _)| s.len())
                .unwrap_or_default()
        })
        .max()
        .unwrap_or_default();
    let write_durations = |col1: &str, durations: Box<dyn Iterator<Item = String>>, total: &str| {
        print!("{}", col1);
        print!(
            "{}",
            " ".repeat(activity_length + shortname_length + 2 - col1.len())
        );
        for d in durations {
            print!(" {:>10}", d);
        }
        println!("  {:>5}", total);
    };

    // date line
    write_durations(
        "",
        Box::new(summaries.iter().map(|(date, ..)| format!("{}", date))),
        "total",
    );

    // start, end, breaks
    write_durations(
        "start",
        Box::new(
            summaries
                .iter()
                .map(|(date, summary)| format_time(&summary.start).to_string()),
        ),
        "",
    );
    write_durations(
        "end",
        Box::new(
            summaries
                .iter()
                .map(|(date, summary)| format_time(&summary.end).to_string()),
        ),
        "",
    );
    write_durations(
        "breaks",
        Box::new(
            summaries
                .iter()
                .map(|(date, summary)| format_duration(&summary.breaks)),
        ),
        "",
    );

    // worktime
    let total_worktime = summaries
        .iter()
        .map(|(date, summary)| summary.work_time)
        .fold(Duration::zero(), |lhs, rhs| lhs + rhs);
    write_durations(
        "worktime",
        Box::new(
            summaries
                .iter()
                .map(|(date, summary)| format_duration(&summary.work_time).to_string()),
        ),
        &format_duration(&total_worktime).to_string(),
    );
    println!("");

    // activities
    for name in activity_names {
        let activity = activities.get(name).unwrap();
        let shortname = activity_map.get(*name).map(|(_, v)| v.first()).flatten();
        let maybe_with_shortname = shortname.map(|s| {
            format!(
                "{}{} ={}",
                name,
                " ".repeat(activity_length - name.len()),
                s,
            )
        });
        let full_name = match maybe_with_shortname {
            None => *name,
            Some(ref with_shortname) => with_shortname,
        };
        write_durations(
            full_name,
            Box::new(summaries.iter().map(move |(date, ..)| {
                activity
                    .durations
                    .get(date)
                    .map_or_else(|| "-".to_string(), |d| format_duration(d))
            })),
            &format_duration(&activity.total),
        );
    }
}
