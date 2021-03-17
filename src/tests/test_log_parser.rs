use crate::error::{TTError, TTErrorKind};
use crate::log_parser::*;
use chrono::NaiveTime;
use std::io;

// tests for normal behaviour

#[test]
fn normal_block() {
    match Block::from_line(Ok("11:30 _test bla foo".to_string())) {
        Ok(Block::NormalBlock(data)) => {
            assert_eq!(data.start, NaiveTime::from_hms(11, 30, 0));
            assert_eq!(data.activity, "_test");
            assert_eq!(data.tags, vec!["bla", "foo"]);
            assert_eq!(data.distribute, true);
        }
        _ => panic!("should have been a NormalBlock"),
    }
}

#[test]
fn comment_block() {
    match Block::from_line(Ok("# bla".to_string())) {
        Ok(Block::CommentBlock(())) => (),
        _ => panic!("should have been a comment block"),
    }
}
#[test]
fn test_time_correction() {
    match Block::from_line(Ok("10:30 really 7:15".to_string())) {
        Ok(Block::TimeCorrection(time)) => assert_eq!(time, NaiveTime::from_hms(7, 15, 0)),
        _ => panic!("should have been a time correction block"),
    }
}

#[test]
fn test_really_block() {
    match Block::from_line(Ok("0:15 really email".to_string())) {
        Ok(Block::ReallyBlock(data)) => {
            assert_eq!(data.start, NaiveTime::from_hms(0, 15, 0));
            assert_eq!(data.activity, "email");
            assert_eq!(data.tags.is_empty(), true);
            assert_eq!(data.distribute, false);
        }
        _ => panic!("should have been a really block"),
    }
}

// tests for error results

#[test]
fn test_io_error() {
    let error = io::Error::new(io::ErrorKind::Other, "message");
    match Block::from_line(Err(error)) {
        Err(TTError {
            kind: TTErrorKind::IoError(err),
            context: _,
        }) => {
            assert_eq!(err.kind(), io::ErrorKind::Other);
            assert_eq!(err.to_string(), "message");
        }
        _ => panic!("should have been an (io) error"),
    }
}

#[test]
fn test_parse_time_error() {
    match Block::from_line(Ok("25:00 bla".to_string())) {
        Err(TTError {
            kind: TTErrorKind::ParseError(msg, line),
            context: _,
        }) => {
            assert_eq!(line, "25:00 bla");
            assert_eq!(msg, "cannot parse start time");
        }
        _ => panic!("should have been a parse error"),
    }
}

#[test]
fn test_parse_no_activity_error() {
    match Block::from_line(Ok("8:01    ".to_string())) {
        Err(TTError {
            kind: TTErrorKind::ParseError(msg, _line),
            context: _,
        }) => {
            assert_eq!(msg, "Line does not contain at least 2 words");
        }
        res @ _ => panic!("should have been a parse error, but was {:?}", res),
    }
}

#[test]
fn test_really_no_activity_error() {
    match Block::from_line(Ok("10:00 really  ".to_string())) {
        Err(TTError {
            kind: TTErrorKind::ParseError(msg, _line),
            context: _,
        }) => {
            assert_eq!(msg, "really block does not contain an activity");
        }
        res @ _ => panic!("should have been a parse error, but was {:?}", res),
    }
}

#[test]
fn test_timecorrection_error() {
    match Block::from_line(Ok("10:00 really 09:00 foo".to_string())) {
        Err(TTError {
            kind: TTErrorKind::ParseError(msg, _line),
            context: _,
        }) => {
            assert_eq!(
                msg,
                "time correction cannot have further data, i.e. use only <time> really <time>"
            );
        }
        res @ _ => panic!("should have been a parse error, but was {:?}", res),
    }
}
