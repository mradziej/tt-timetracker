use chrono::Duration;
use chrono::NaiveTime;
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::mem;

use super::log_parser::{self, Block, BlockData};
use crate::error::TTError;

// use crate::SummaryFormat::{Short, Long};

// name -> (duration, map(tag -> duration))
pub(crate) type ActivityHashMap = HashMap<String, (Duration, BTreeMap<String, Duration>)>;

#[derive(Debug)]
pub struct Summary {
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub breaks: Duration,
    pub work_time: Duration,
    pub activities: ActivityHashMap,
    pub distribute: Duration,
}

struct ProgressData {
    summary: Summary,
    last: BlockData, // the data of the last NormalBlock, which has not ended yet
    prev: Option<BlockData>, // the data of the previous NormalBlock, already in calculation
}

pub struct BlockCollector {
    state: Option<ProgressData>,
}

pub struct CollectResult {
    pub summary: Summary,
    pub final_activity: String,
    pub final_shortname: Option<String>,
    pub final_start: NaiveTime,
}

/// consumes Blocks and builds up a summary that can be used for a report.
///
/// BlockCollector::new() initializes the collector,
/// BlockCollector::add() adds a block,
/// BlockCollector::finalize() finishes the collection and returns the results.
///
/// ```
/// use chrono::NaiveTime;
/// use timetracker::log_parser::{Block, BlockData};
/// use timetracker::collector::{BlockCollector, CollectResult};
///
/// let mut collector = BlockCollector::new();
/// collector.add(Block::NormalBlock(BlockData{
///     start: NaiveTime::from_hms(8,30,0),
///     activity: "email".to_string(),
///     tags: vec![],
///     distribute: false,
/// }));
/// collector.add(Block::NormalBlock(BlockData {
///     start: NaiveTime::from_hms(9,15,0),
///     activity: "break".to_string(),
///     tags: vec![],
///     distribute: false,
/// }));
/// let CollectResult{summary, final_activity, final_shortname, final_start} = collector.finalize(None).unwrap();
/// assert_eq!(summary.work_time.num_minutes(), 45);
/// assert_eq!(final_activity, "break");
/// assert!(final_shortname.is_none());
/// assert_eq!(final_start, NaiveTime::from_hms(9,15,0));
/// ```
impl BlockCollector {
    pub fn new() -> BlockCollector {
        BlockCollector { state: None }
    }

    pub fn add(&mut self, block: Block) {
        // TODO: check that start time is monotonic increasing
        // Idea: Instead of Option<ProgressData>, have an enum (Empty, Progress, Error)
        // and Error has a list of TTErrors. finalize can then return this.
        match &mut self.state {
            None => {
                match block {
                    Block::ReallyBlock(_) => (),
                    Block::TimeCorrection(_) => (),
                    Block::CommentBlock(_) => (),
                    Block::NormalBlock(b) => {
                        let summary = Summary {
                            start: b.start,
                            end: b.start,
                            breaks: Duration::zero(),
                            work_time: Duration::zero(), // includes "distribute"
                            activities: HashMap::new(),
                            distribute: Duration::zero(),
                        };

                        self.state = Some(ProgressData {
                            summary,
                            last: b,
                            prev: None,
                        })
                    }
                }
            }
            Some(data) => match block {
                Block::CommentBlock(_) => (),
                Block::TimeCorrection(real_start) => {
                    let diff = real_start - data.last.start; // positive means prev ended later
                                                             // unwrap: We only hold temporarily 2 references on data.last, at the end of this fn
                    data.last.start = real_start;
                    data.summary.end = real_start;
                    match &data.prev {
                        Some(before) => {
                            // unwrap: the activity has been added to the hash when `before` was added
                            let (act_duration, act_tags) =
                                data.summary.activities.get_mut(&before.activity).unwrap();
                            *act_duration = *act_duration + diff;
                            for duration in act_tags.values_mut() {
                                *duration = *duration + diff;
                            }
                            if log_parser::is_break(&before.activity) {
                                data.summary.breaks = data.summary.breaks + diff;
                            } else {
                                data.summary.work_time = data.summary.work_time + diff;
                            }
                            if before.distribute {
                                data.summary.distribute = data.summary.distribute + diff;
                            }
                        }
                        None => {
                            data.summary.start = real_start;
                        }
                    }

                    // the activity that ended with the start of the data.last block needs correction.
                    // it ended at a different time.
                    //
                    // now we need to correct the block that started before the block in data.last
                }
                Block::ReallyBlock(b) => {
                    data.last = BlockData {
                        start: data.last.start,
                        activity: b.activity,
                        tags: b.tags,
                        distribute: b.distribute,
                    };
                }
                Block::NormalBlock(b) => {
                    assert!(b.start >= data.last.start);
                    let duration = b.start - data.last.start;
                    data.summary.end = b.start;
                    if log_parser::is_break(&data.last.activity) {
                        data.summary.breaks = data.summary.breaks + duration;
                    } else {
                        data.summary.work_time = data.summary.work_time + duration;
                    }
                    if data.last.distribute {
                        data.summary.distribute = data.summary.distribute + duration;
                    }
                    match data.summary.activities.get_mut(&data.last.activity) {
                        Some((old_duration, duration_map)) => {
                            *old_duration = *old_duration + duration;
                            for tag in data.last.tags.iter() {
                                let old_tag_duration =
                                    duration_map.get(tag).map_or_else(Duration::zero, |d| *d);
                                duration_map.insert(tag.clone(), old_tag_duration + duration);
                            }
                        }
                        None => {
                            let duration_map = data
                                .last
                                .tags
                                .iter()
                                .map(|tag| (tag.clone(), duration))
                                .collect();
                            data.summary
                                .activities
                                .insert(data.last.activity.to_string(), (duration, duration_map));
                        }
                    }
                    // this sets: data.prev = data.last; data.last=b
                    // but avoids duplicate references to data.last.
                    // unwrap is guaranteed to work since we just set data.prev to something.
                    data.prev = Some(b);
                    mem::swap(data.prev.as_mut().unwrap(), &mut data.last);
                }
            },
        }
    }

    // summary, last_activity, last_shortname, last_start
    pub fn finalize(mut self, at: Option<&NaiveTime>) -> Option<CollectResult> {
        let (last_activity, last_shortname, last_start) = match &self.state {
            None => (None, None, None),
            Some(data) => {
                let save = (
                    Some(data.last.activity.to_string()),
                    Some(
                        data.last
                            .tags
                            .iter()
                            .find(|s| s.starts_with("="))
                            .map(|s| s.to_string()),
                    ),
                    Some(data.last.start),
                );
                if let Some(time) = at {
                    self.add(Block::NormalBlock(BlockData {
                        start: time.clone(),
                        activity: "break".to_string(),
                        tags: vec![],
                        distribute: false,
                    }))
                }
                save
            }
        };
        match self.state {
            None => None,
            Some(data) => {
                self.state = None;
                Some(CollectResult {
                    summary: data.summary,
                    final_activity: last_activity.unwrap(),
                    final_shortname: last_shortname.unwrap(),
                    final_start: last_start.unwrap(),
                })
            }
        }
    }
}
/// convenience function: parses logs from the string iterator, collect them, finalize.
/// Returns a tuple (summary, final activity, final shortname, final start time)
///
/// ```
/// use timetracker::collector::{self, CollectResult};
/// use timetracker::log_parser::{Block, BlockData};
/// use timetracker::get_logfile_name;
/// use chrono::{Local, NaiveTime};
/// use std::vec;
/// use std::io::{self, BufReader, BufRead};
/// use std::fs;
/// let date = chrono::Local::now().date().naive_local();
/// let pathname = get_logfile_name(& date);
/// # let pathname="/dev/null";
/// let reader = BufReader::new(fs::File::open(pathname).expect("Cannot open read for reading")).lines();
/// # let reader = vec![Ok("8:00 write-tests =test".to_string())].into_iter();
/// let CollectResult{summary, final_activity, final_shortname, final_start} =
///     collector::collect_blocks(reader, Some(&NaiveTime::from_hms(12,0,0))).unwrap().unwrap();
/// assert_eq!(summary.work_time.num_minutes(), 4*60);
/// assert_eq!(final_activity, "write-tests");
/// assert_eq!(final_shortname, Some("=test".to_string()));
/// assert_eq!(final_start, NaiveTime::from_hms(8,0,0));
/// ```
pub fn collect_blocks<T: Iterator<Item = io::Result<String>>>(
    line_iterator: T,
    add_ending_at: Option<&NaiveTime>,
) -> Result<Option<CollectResult>, TTError> {
    let blocks = line_iterator.map(log_parser::Block::from_line);
    let mut collector = BlockCollector::new();
    for block in blocks {
        match block {
            Ok(b) => collector.add(b),
            Err(err) => return Err(err),
        }
    }
    Ok(collector.finalize(add_ending_at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TTErrorKind;
    use crate::utils::setup_line_reader;
    use std::iter::{self, FromIterator};

    //   - start with timecorrection
    //   - start with really
    //   - time goes backward

    // setup adds a normal block at 8:30 "setup"
    fn setup_test() -> BlockCollector {
        let mut collector = BlockCollector::new();
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(8, 30, 0),
            activity: "setup".to_string(),
            tags: vec![],
            distribute: false,
        }));
        collector
    }

    #[test]
    fn normal_block() {
        let mut collector = setup_test();
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(10, 0, 0),
            activity: "end".to_string(),
            tags: vec![],
            distribute: false,
        }));
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collector.finalize(None).unwrap();
        let expected_activities = HashMap::from_iter(iter::once((
            "setup".to_string(),
            (Duration::minutes(90), BTreeMap::new()),
        )));
        assert_eq!(summary.start, NaiveTime::from_hms(8, 30, 0));
        assert_eq!(summary.end, NaiveTime::from_hms(10, 0, 0));
        assert_eq!(summary.breaks.num_minutes(), 0);
        assert_eq!(summary.work_time.num_minutes(), 90);
        assert_eq!(summary.activities, expected_activities);
        assert_eq!(summary.distribute.num_minutes(), 0);

        assert_eq!(final_activity, "end");
        assert!(final_shortname.is_none());
        assert_eq!(final_start, NaiveTime::from_hms(10, 0, 0));
    }

    #[test]
    fn really_block() {
        let mut collector = setup_test();
        collector.add(Block::ReallyBlock(BlockData {
            start: NaiveTime::from_hms(9, 0, 0),
            activity: "correct".to_string(),
            tags: vec!["=short".to_string(), "tag1".to_string()],
            distribute: false,
        }));
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collector
            .finalize(Some(&NaiveTime::from_hms(11, 0, 0)))
            .unwrap();
        // correct from 8:30 - 11.00
        let expected_duration = 2 * 60 + 30;
        let expected_activities = HashMap::from_iter(iter::once((
            "correct".to_string(),
            (
                Duration::minutes(expected_duration),
                BTreeMap::from_iter(vec![
                    ("=short".to_string(), Duration::minutes(expected_duration)),
                    ("tag1".to_string(), Duration::minutes(expected_duration)),
                ]),
            ),
        )));
        assert_eq!(summary.start, NaiveTime::from_hms(8, 30, 0));
        assert_eq!(summary.end, NaiveTime::from_hms(11, 0, 0));
        assert_eq!(summary.breaks.num_minutes(), 0);
        assert_eq!(summary.work_time.num_minutes(), expected_duration);
        assert_eq!(summary.activities, expected_activities);
        assert_eq!(summary.distribute.num_minutes(), 0);

        assert_eq!(final_activity, "correct");
        assert_eq!(final_shortname, Some("=short".to_string()));
        assert_eq!(final_start, NaiveTime::from_hms(8, 30, 0));
    }

    #[test]
    fn timecorrection_block() {
        let mut collector = setup_test();
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(10, 0, 0),
            activity: "_something".to_string(),
            tags: vec!["other".to_string()],
            distribute: true,
        }));
        collector.add(Block::TimeCorrection(NaiveTime::from_hms(11, 0, 0)));
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collector
            .finalize(Some(&NaiveTime::from_hms(11, 30, 0)))
            .unwrap();
        // setup from 08:30 - 11.00
        // _something from 11-00 - 11.30
        let expected_activities = HashMap::from_iter(vec![
            (
                "setup".to_string(),
                (Duration::minutes(2 * 60 + 30), BTreeMap::new()),
            ),
            (
                "_something".to_string(),
                (
                    Duration::minutes(30),
                    BTreeMap::from_iter(iter::once(("other".to_string(), Duration::minutes(30)))),
                ),
            ),
        ]);
        assert_eq!(summary.start, NaiveTime::from_hms(8, 30, 0));
        assert_eq!(summary.end, NaiveTime::from_hms(11, 30, 0));
        assert_eq!(summary.breaks.num_minutes(), 0);
        assert_eq!(summary.work_time.num_minutes(), 3 * 60);
        assert_eq!(summary.activities, expected_activities);
        assert_eq!(summary.distribute.num_minutes(), 30);

        assert_eq!(final_activity, "_something");
        assert!(final_shortname.is_none());
        assert_eq!(final_start, NaiveTime::from_hms(11, 0, 0));
    }

    #[test]
    fn comment_block() {
        let mut collector = setup_test();
        collector.add(Block::CommentBlock(()));
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(10, 0, 0),
            activity: "end".to_string(),
            tags: vec![],
            distribute: false,
        }));
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collector.finalize(None).unwrap();
        let expected_activities = HashMap::from_iter(iter::once((
            "setup".to_string(),
            (Duration::minutes(90), BTreeMap::new()),
        )));
        assert_eq!(summary.start, NaiveTime::from_hms(8, 30, 0));
        assert_eq!(summary.end, NaiveTime::from_hms(10, 0, 0));
        assert_eq!(summary.breaks.num_minutes(), 0);
        assert_eq!(summary.work_time.num_minutes(), 90);
        assert_eq!(summary.activities, expected_activities);
        assert_eq!(summary.distribute.num_minutes(), 0);

        assert_eq!(final_activity, "end");
        assert!(final_shortname.is_none());
        assert_eq!(final_start, NaiveTime::from_hms(10, 0, 0));
    }

    #[test]
    fn no_block() {
        let collector = BlockCollector::new();
        assert!(collector
            .finalize(Some(&NaiveTime::from_hms(17, 0, 0)))
            .is_none());
    }

    #[test]
    fn one_block_with_final_time() {
        let collector = setup_test();
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collector
            .finalize(Some(&NaiveTime::from_hms(17, 0, 0)))
            .unwrap();
        // println!("{:?}", result);
        // 8.30-17.00: setup
        let expected_work_time = 8 * 60 + 30;
        let expected_activities = HashMap::from_iter(iter::once((
            "setup".to_string(),
            (Duration::minutes(expected_work_time), BTreeMap::new()),
        )));
        assert_eq!(summary.start, NaiveTime::from_hms(8, 30, 0));
        assert_eq!(summary.end, NaiveTime::from_hms(17, 0, 0));
        assert_eq!(summary.breaks.num_minutes(), 0);
        assert_eq!(summary.work_time.num_minutes(), expected_work_time);
        assert_eq!(summary.activities, expected_activities);
        assert_eq!(summary.distribute.num_minutes(), 0);

        assert_eq!(final_activity, "setup");
        assert!(final_shortname.is_none());
        assert_eq!(final_start, NaiveTime::from_hms(8, 30, 0));
    }

    #[test]
    fn one_block_without_final_time() {
        let collector = setup_test();
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collector.finalize(None).unwrap();
        // println!("{:?}", result);
        // 8.30-8.30: setup
        let expected_work_time = 0;
        let expected_activities = HashMap::new();
        assert_eq!(summary.start, NaiveTime::from_hms(8, 30, 0));
        assert_eq!(summary.end, NaiveTime::from_hms(8, 30, 0));
        assert_eq!(summary.breaks.num_minutes(), 0);
        assert_eq!(summary.work_time.num_minutes(), expected_work_time);
        assert_eq!(summary.activities, expected_activities);
        assert_eq!(summary.distribute.num_minutes(), 0);

        assert_eq!(final_activity, "setup");
        assert!(final_shortname.is_none());
        assert_eq!(final_start, NaiveTime::from_hms(8, 30, 0));
    }

    #[test]
    fn timecorrection_as_first_block() {
        let mut collector = BlockCollector::new();
        collector.add(Block::TimeCorrection(NaiveTime::from_hms(10, 0, 0)));
        assert!(collector
            .finalize(Some(&NaiveTime::from_hms(17, 0, 0)))
            .is_none());
    }

    #[test]
    fn timecorrection_coverage() {
        let mut collector = setup_test();
        collector.add(Block::TimeCorrection(NaiveTime::from_hms(9, 0, 0)));
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(9, 30, 0),
            activity: "setup".to_string(),
            tags: vec!["sometag".to_string()],
            distribute: false,
        }));
        collector.add(Block::TimeCorrection(NaiveTime::from_hms(11, 0, 0)));
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(11, 5, 0),
            activity: "break".to_string(),
            tags: vec![],
            distribute: false,
        }));
        collector.add(Block::TimeCorrection(NaiveTime::from_hms(11, 10, 0)));
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(11, 20, 0),
            activity: "_distribute".to_string(),
            tags: vec![],
            distribute: true,
        }));
        collector.add(Block::TimeCorrection(NaiveTime::from_hms(11, 22, 0)));
        collector.add(Block::NormalBlock(BlockData {
            start: NaiveTime::from_hms(11, 32, 0),
            activity: "_distribute".to_string(),
            tags: vec!["sometag".to_string()],
            distribute: true,
        }));
        collector.add(Block::TimeCorrection(NaiveTime::from_hms(11, 23, 0)));
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collector
            .finalize(Some(&NaiveTime::from_hms(12, 0, 0)))
            .unwrap();

        // println!("{:?}", result);
        //  9.00 - 11:00 setup
        // 11.00 - 11.10 setup sometag
        // 11.10 - 11.22 break
        // 11.22 - 11.23 _distribute
        // 11.23 - 12.00 _distribute sometag
        let expected_work_time = 3 * 60 - 12;
        let expected_activities = HashMap::from_iter(vec![
            (
                ("setup".to_string()),
                (
                    Duration::minutes(2 * 60 + 10),
                    BTreeMap::from_iter(iter::once(("sometag".to_string(), Duration::minutes(10)))),
                ),
            ),
            ((
                "break".to_string(),
                (Duration::minutes(12), BTreeMap::new()),
            )),
            ((
                "_distribute".to_string(),
                (
                    Duration::minutes(38),
                    BTreeMap::from_iter(iter::once(("sometag".to_string(), Duration::minutes(37)))),
                ),
            )),
        ]);
        assert_eq!(summary.start, NaiveTime::from_hms(9, 0, 0));
        assert_eq!(summary.end, NaiveTime::from_hms(12, 0, 0));
        assert_eq!(summary.breaks.num_minutes(), 12);
        assert_eq!(summary.work_time.num_minutes(), expected_work_time);
        assert_eq!(summary.activities, expected_activities);
        assert_eq!(summary.distribute.num_minutes(), 38);

        assert_eq!(final_activity, "_distribute");
        assert!(final_shortname.is_none());
        assert_eq!(final_start, NaiveTime::from_hms(11, 23, 0));
    }

    #[test]
    fn test_collect_blocks() {
        let lines = setup_line_reader(vec![
            "8:30 write-tests =test",
            "10:05 really 10:00",
            "10:30 blabla",
        ]);
        let CollectResult {
            summary,
            final_activity,
            final_shortname,
            final_start,
        } = collect_blocks(lines, Some(&NaiveTime::from_hms(12, 0, 0)))
            .unwrap()
            .unwrap();
        assert_eq!(summary.work_time.num_minutes(), 2 * 60);
        assert_eq!(final_activity, "blabla");
        assert_eq!(final_shortname, None);
        assert_eq!(final_start, NaiveTime::from_hms(10, 30, 0));
    }

    #[test]
    fn test_collect_blocks_error() {
        let lines = setup_line_reader(vec!["8:30 write-tests", "10:05 really 10:00 parsing-error"]);
        match collect_blocks(lines, None) {
            Err(TTError {
                kind: TTErrorKind::ParseError(_msg, line),
                context: _,
            }) => assert_eq!(line, "10:05 really 10:00 parsing-error"),
            _ => panic!("Parse Error expected"),
        }
    }
}
