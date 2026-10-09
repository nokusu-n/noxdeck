mod common;
use common::overfast::{Reply, Stub};
use noxdeck_lib::ratings::overfast::{Client, Report, TIMEOUT};
use std::time::{Duration, Instant};

const TAG: &str = "ExamplePlayer#12345";
const FULL: &str = r#"{"competitive":{"pc":{"tank":{"division":"gold","tier":3},"damage":{"division":"emerald","tier":1},"support":{"division":"champion","tier":5}},"console":{"tank":{"division":"bronze","tier":5}}}}"#;

#[test]
fn requests_the_case_preserved_battletag_path_and_reads_only_pc_ratings() {
    let stub = Stub::new(); stub.set(TAG, Reply::json(FULL));
    let report = Client::new(&stub.url).fetch(TAG);
    let Report::Ratings(values) = report else { panic!("expected ratings"); };
    for (value, rank, division) in [(values[0].as_ref(),"gold",3),(values[1].as_ref(),"emerald",1),(values[2].as_ref(),"champion",5)] {
        assert_eq!(value.unwrap().rank, rank); assert_eq!(value.unwrap().division, Some(division));
    }
    assert!(stub.requests.lock().unwrap()[0].starts_with("GET /players/ExamplePlayer-12345/summary HTTP/1.1\r\n"));
}

#[test]
fn missing_null_unknown_and_malformed_roles_are_not_reports() {
    let stub = Stub::new(); let client = Client::new(&stub.url);
    for body in [r#"{"competitive":{"pc":{"tank":null,"damage":{"division":"future","tier":2},"support":{"division":"gold","tier":6}}}}"#, r#"{"competitive":{"console":{"tank":{"division":"gold","tier":2}}}}"#, "{}", "garbage"] {
        stub.set(TAG, Reply::json(body));
        assert_eq!(client.fetch(TAG), Report::Ratings([None, None, None]));
    }
}

#[test]
fn failures_are_silent_and_rate_limits_end_the_refresh_without_retry() {
    let stub = Stub::new(); let client = Client::new(&stub.url);
    for status in [404,403,500,429,503] {
        stub.set(TAG, Reply { status, body: FULL.into(), delay: Duration::ZERO });
        assert_eq!(client.fetch(TAG), if [429,503].contains(&status) { Report::Stop } else { Report::Ratings([None,None,None]) });
    }
    assert_eq!(stub.count(), 5);
}

#[test]
fn the_whole_request_has_a_fifteen_second_bound() {
    assert_eq!(TIMEOUT, Duration::from_secs(15));
    let stub = Stub::new(); stub.set(TAG, Reply { delay: Duration::from_millis(500), ..Reply::json(FULL) });
    let client = Client::with_timeout(&stub.url, Duration::from_millis(80));
    let start = Instant::now();
    assert_eq!(client.fetch(TAG), Report::Ratings([None,None,None]));
    assert!(start.elapsed() < Duration::from_millis(400));
    assert_eq!(stub.count(), 1);
}
