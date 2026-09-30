//! Terminal color reports interleaved with typed input surface as events and never become keys.

use std::io::Write;
use std::time::{Duration, Instant};

use super::tests::source_with_input;
use super::Parser;
use crate::event::source::EventSource;
use crate::event::{ColorReport, ColorScheme, Event, InternalEvent, KeyCode};
use crate::style::Color;

const INTERLEAVED_REPORTS: &[u8] =
    b"a\x1b]11;rgb:1111/1111/1111\x07b\x1b]10;rgb:eeee/eeee/eeee\x1b\\c\x1b[?997;2nd\x1b[?997;1ne";

fn interleaved_events() -> Vec<Event> {
    vec![
        Event::Key(KeyCode::Char('a').into()),
        Event::ColorReport(ColorReport::BackgroundColor(Color::Rgb {
            r: 17,
            g: 17,
            b: 17,
        })),
        Event::Key(KeyCode::Char('b').into()),
        Event::ColorReport(ColorReport::ForegroundColor(Color::Rgb {
            r: 238,
            g: 238,
            b: 238,
        })),
        Event::Key(KeyCode::Char('c').into()),
        Event::ColorReport(ColorReport::ColorScheme(ColorScheme::Light)),
        Event::Key(KeyCode::Char('d').into()),
        Event::ColorReport(ColorReport::ColorScheme(ColorScheme::Dark)),
        Event::Key(KeyCode::Char('e').into()),
    ]
}

fn public_events(parser: Parser) -> Vec<Event> {
    parser.filter_map(InternalEvent::into_event).collect()
}

#[test]
fn color_reports_surface_between_keys_at_any_split() {
    for split in 0..=INTERLEAVED_REPORTS.len() {
        let mut parser = Parser::default();
        parser.advance(&INTERLEAVED_REPORTS[..split], /*more*/ true);
        parser.advance(&INTERLEAVED_REPORTS[split..], /*more*/ false);
        assert_eq!(public_events(parser), interleaved_events(), "split {split}");
    }
}

#[test]
fn color_reports_fed_byte_by_byte_keep_keys_intact() {
    let mut parser = Parser::default();
    for byte in INTERLEAVED_REPORTS {
        parser.advance(std::slice::from_ref(byte), /*more*/ true);
    }
    assert_eq!(public_events(parser), interleaved_events());
}

#[test]
fn unrecognized_color_replies_produce_no_events() {
    let mut parser = Parser::default();
    for byte in b"a\x1b]11;?\x07b\x1b[?997;3nc\x1b[?10nd" {
        parser.advance(std::slice::from_ref(byte), /*more*/ true);
    }
    assert_eq!(
        public_events(parser),
        vec![
            Event::Key(KeyCode::Char('a').into()),
            Event::Key(KeyCode::Char('b').into()),
            Event::Key(KeyCode::Char('c').into()),
            Event::Key(KeyCode::Char('d').into()),
        ]
    );
}

/// Deliver `input` one byte per read through a live source and collect the public events.
fn read_live_byte_by_byte(input: &[u8]) -> Vec<Event> {
    let (mut source, mut writer) = source_with_input();
    let mut events = Vec::new();
    for byte in input {
        writer.write_all(std::slice::from_ref(byte)).unwrap();
        while let Some(event) = source.try_read(Some(Duration::from_millis(1))).unwrap() {
            events.extend(event.into_event());
        }
    }
    events
}

#[test]
fn live_color_reports_split_after_escape_keep_keys_intact() {
    assert_eq!(
        read_live_byte_by_byte(INTERLEAVED_REPORTS),
        interleaved_events()
    );
}

#[test]
fn separate_live_escape_presses_are_preserved() {
    let (mut source, mut writer) = source_with_input();
    writer.write_all(b"\x1b").unwrap();
    assert_eq!(
        source.try_read(Some(Duration::from_millis(1))).unwrap(),
        None
    );
    writer.write_all(b"\x1b").unwrap();
    let escape = Some(InternalEvent::Event(Event::Key(KeyCode::Esc.into())));
    assert_eq!(
        [
            source.try_read(Some(Duration::from_millis(100))).unwrap(),
            source.try_read(Some(Duration::from_millis(100))).unwrap(),
        ],
        [escape.clone(), escape]
    );
}

#[test]
fn expired_live_escape_is_a_key_before_later_input() {
    let (mut source, mut writer) = source_with_input();
    let key = |code: KeyCode| Some(InternalEvent::Event(Event::Key(code.into())));
    writer.write_all(b"a\x1b").unwrap();
    assert_eq!(
        source.try_read(Some(Duration::from_millis(10))).unwrap(),
        key(KeyCode::Char('a'))
    );

    // A slow consumer polls again only after the Escape window has passed.
    source.parser.pending_escape_deadline = Some(Instant::now());
    writer.write_all(b"x").unwrap();
    assert_eq!(
        [
            source.try_read(Some(Duration::from_millis(10))).unwrap(),
            source.try_read(Some(Duration::from_millis(10))).unwrap(),
        ],
        [key(KeyCode::Esc), key(KeyCode::Char('x'))]
    );
}
