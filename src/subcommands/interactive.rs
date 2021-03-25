use crate::error::{TTError, TTErrorKind};
use crate::log_parser::{is_distributable, is_start, Block, BlockData};
use crate::utils::FileProxy;
use crate::{collector, subcommands};
use chrono::{DateTime, Local};
use itertools::Itertools;
use std::io::{BufRead, Write};
use text_io::read;

pub(crate) fn run<'a, R: BufRead, W: Write, F: FileProxy<R, W>>(
    now: &DateTime<Local>,
    default_logfile: &F,
    activitiesfile: &F,
) -> Result<i32, TTError> {
    interactive(now, default_logfile, activitiesfile)?;
    Ok(0)
}

// list the activitiesfile, numbered
// list other activities today
// -> number => enter that
// r -> resume, shows list
// e -> edit logfile
// a -> edit activitiesfile
// +xxx -> add this log file
// return -> provide summary with numbers and query again.
fn interactive<R: BufRead, W: Write, F: FileProxy<R, W>>(
    now: &DateTime<Local>,
    default_logfile: &F,
    activitiesfile: &F,
) -> Result<(), TTError> {
    let timestamp = now.time();
    let activity_map = subcommands::add::read_activities(activitiesfile.reader()?)
        .expect("Cannot read config file");
    let collected =
        match collector::collect_blocks(default_logfile.reader()?.lines(), Some(&timestamp)) {
            Ok(Some(collected)) => Some(collected),
            Ok(None) => None,
            Err(err) => return Err(err),
        };
    let really = collected
        .as_ref()
        .map_or(false, |c| is_start(&c.final_activity));
    println!("{:?}", activity_map);
    let activities_sorted: Vec<_> = activity_map
        .iter()
        .filter(|(k, (v, _tags))| *k != v)
        .sorted()
        .enumerate()
        .collect();
    if let Some(real_collected) = &collected {
        println!(
            "{}",
            subcommands::report::short_report(&real_collected.summary)
        );
    }
    println!("Configured activities:");
    for (i, (k, _v)) in &activities_sorted {
        println!("{:>2} {}", i, k);
    }
    if really {
        println!("--really implied")
    };
    println!("number, r(esume), e(edit), c(activitiesfile), (+,_)add, (enter) (for day list)? ");
    let cmd: String = read!("{}\n");
    let first_char = cmd.chars().next().unwrap_or('?');
    match first_char {
        '0'..='9' => match cmd
            .parse::<usize>()
            .and_then(|i| Ok(activities_sorted.get(i)))
        {
            Ok(Some((_i, (k, _v)))) => {
                let data = BlockData {
                    start: timestamp,
                    activity: k.to_string(),
                    tags: vec![],
                    distribute: false,
                };
                subcommands::add::add(
                    Block::from_data(data, really),
                    Some(&activity_map),
                    activitiesfile,
                    default_logfile,
                    &timestamp,
                    now,
                )
            }
            _ => Err(TTError::new(TTErrorKind::UsageError(
                "I did not understand your reply, aborting.",
            ))
            .context(format!("in interactive mode, interpreting '{}", cmd))),
        },
        'r' => {
            let resume_stack =
                subcommands::resume::find_resume_activities(default_logfile.reader()?);
            for (i, (activity, tags, _offset)) in resume_stack.iter().enumerate() {
                if !is_start(activity) {
                    println!("{:2>} {} {}", i, activity, tags.join(" "));
                }
            }
            print!("Resume which? ");
            let cmd: String = read!("{}\n");
            let n = cmd
                .parse::<usize>()
                .expect("I did not understand. Aborting.");
            subcommands::resume::resume(
                activitiesfile,
                default_logfile,
                now,
                Some(timestamp),
                n,
                really,
            )
        }
        'e' => subcommands::edit::edit(default_logfile.pathname()),
        'a' => subcommands::edit::edit(activitiesfile.pathname()),
        '+' | '_' => {
            let words: Vec<String> = cmd.split(" ").map(str::to_string).collect();
            let (activity, tags) = (&words[0], &words[1..]);
            let data = BlockData {
                start: timestamp,
                activity: activity.to_string(),
                tags: tags.to_vec(),
                distribute: is_distributable(activity),
            };
            subcommands::add::add(
                Block::from_data(data, really),
                Some(&activity_map),
                activitiesfile,
                default_logfile,
                &timestamp,
                now,
            )
        }
        _ => match collected {
            None => Err(TTError::new(TTErrorKind::UsageError("Nothing logged yet."))
                .context(format!("in interactive mode, interpreting '{}'", cmd))),
            Some(mut collected) => {
                println!("Activities of the day:");
                let report_lines =
                    subcommands::report::long_report_lines(&mut collected.summary, &None);
                for (i, (s, _activity)) in report_lines.iter().enumerate() {
                    println!("{:>2} {}", i, s);
                }
                println!("Which one (pick a number)? ");
                let reply: String = read!("{}\n");
                match reply.parse::<usize>().and_then(|i| Ok(report_lines.get(i))) {
                    Ok(Some((_line, activity))) => {
                        let block = Block::NormalBlock(BlockData {
                            start: timestamp,
                            activity: format!("+{}", activity),
                            tags: vec![],
                            distribute: false,
                        });
                        subcommands::add::add(
                            block,
                            Some(&activity_map),
                            activitiesfile,
                            default_logfile,
                            &timestamp,
                            now,
                        )
                    }
                    _ => Err(TTError::new(TTErrorKind::UsageError(
                        "I don't know what you want from me ... aborting.",
                    ))
                    .context(format!("in interactive mode, interpreting '{}'", reply))),
                }
            }
        }, // _ => print!("Not yet implemented."),
    }
}
