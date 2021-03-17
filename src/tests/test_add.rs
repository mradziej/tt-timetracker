use crate::{
    error::{TTError, TTErrorKind},
    tests::TestData,
};

// fn run_test(setup: Fn(&mut FakeFile, &mut FakeFile, &mut FakeFile, &mut NaiveDateTime, &mut String), )
// integration tests for the add subcommand
#[test]
fn add_simple() {
    let data = TestData::new()
        .with_args("+test-simple tag-1 tag-2")
        .run()
        .unwrap();
    assert!(data.activitiesfile.close().unwrap().is_empty());
    assert_eq!(
        data.logfile.close().unwrap(),
        "08:00 test-simple tag-1 tag-2\n"
    );
}

#[test]
fn add_unknown() {
    match TestData::new().with_args("test-unknown").run() {
        Err(TTError {
            kind: TTErrorKind::UsageError(_),
            context: _,
        }) => (),
        _ => panic!("Expected a TTError::UsageError"),
    }
}

#[test]
fn add_force() {
    let data = TestData::new()
        .with_args("+test-simple =new-name sometag")
        .run()
        .unwrap();
    assert_eq!(
        data.logfile.close().unwrap(),
        "08:00 test-simple =new-name sometag\n"
    );
    assert_eq!(
        data.activitiesfile.close().unwrap(),
        "new-name test-simple sometag\n"
    );
}

#[test]
fn add_really() {
    let data = TestData::new()
        .with_args("--really +whatever")
        .run()
        .unwrap();
    assert!(data.activitiesfile.close().unwrap().is_empty());
    assert_eq!(data.logfile.close().unwrap(), "08:00 really whatever\n");
}

#[test]
fn add_with_manual_time() {
    let data = TestData::new()
        .with_args("--time 11:30 +whatever")
        .run()
        .unwrap();
    assert!(data.activitiesfile.close().unwrap().is_empty());
    assert_eq!(data.logfile.close().unwrap(), "08:00 11:30 whatever\n");
}

#[test]
fn add_time_correction() {
    let data = TestData::new()
        .with_args("--time 8:15 --really")
        .run()
        .unwrap();
    assert!(data.activitiesfile.close().unwrap().is_empty());
    assert_eq!(data.logfile.close().unwrap(), "08:00 really 08:15\n");
}

#[test]
fn add_known_activity() {
    let data = TestData::new()
        .with_args("shortcut tag-1")
        .write_activitiesfile("shortcut something tag-2\n".to_string())
        .run()
        .unwrap();
    assert_eq!(
        data.activitiesfile.close().unwrap(),
        "shortcut something tag-2\n"
    );
    assert_eq!(
        data.logfile.close().unwrap(),
        "08:00 something =shortcut tag-1 tag-2\n"
    );
}

#[test]
fn add_expand_prefix() {
    let data = TestData::new()
        .with_args("123")
        .write_activitiesfile("PREFIX-123".to_string())
        .write_configfile("prefix = \"PREFIX\"\n".to_string())
        .run()
        .unwrap();
    assert_eq!(data.logfile.close().unwrap(), "08:00 PREFIX-123\n");
}
