use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::catalog::books::Book as CatalogBook;
use crate::config::ReaderSettings;
use crate::reader as domain;
use crate::tui::help::{Binding, Section as HelpSection};
use crate::tui::reader;
use crate::tui::widgets::centered_rect;

// Plain ASCII frames so the spinner survives any terminal/font over SSH.
const SPINNER: [&str; 4] = ["|", "/", "-", "\\"];

const LOADING_BINDINGS: &[Binding] = &[Binding {
    keys: "Esc",
    desc: "cancel opening",
}];

pub struct State {
    catalog_dir: PathBuf,
    book: CatalogBook,
    settings: ReaderSettings,
    rx: Receiver<Result<domain::Book, String>>,
    frame: usize,
}

pub enum LoadingAction {
    None,
    Cancelled,
    Ready(Box<reader::State>),
    Failed(String),
}

pub fn spawn(catalog_dir: PathBuf, book: CatalogBook, settings: ReaderSettings) -> State {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_dir = catalog_dir.clone();
    let worker_book = book.clone();
    std::thread::spawn(move || {
        // The worker only converts (and reads/writes the reader cache). DB
        // access for the reading-progress restore stays on the main thread.
        // catch_unwind is a safety net on top of the per-format catches in
        // `reader::pdf` / `reader::mobi`: an unforeseen panic must surface as
        // an error, not abort the process.
        let result = catch_unwind(AssertUnwindSafe(|| {
            domain::open(&worker_dir, &worker_book, reader::HTML_RENDER_WIDTH)
        }));
        let message = match result {
            Ok(Ok(book)) => Ok(book),
            Ok(Err(err)) => Err(err.to_string()),
            Err(_) => Err("conversion crashed; the file is likely malformed".to_string()),
        };
        // A send error means the user cancelled and dropped the receiver; the
        // result is simply discarded.
        let _ = tx.send(message);
    });
    State {
        catalog_dir,
        book,
        settings,
        rx,
        frame: 0,
    }
}

pub fn advance(state: &mut State) -> LoadingAction {
    match state.rx.try_recv() {
        Ok(Ok(book)) => LoadingAction::Ready(Box::new(reader::State::open(
            state.catalog_dir.clone(),
            state.book.id,
            book,
            state.settings,
        ))),
        Ok(Err(detail)) => LoadingAction::Failed(failure_message(&state.book.title, &detail)),
        Err(TryRecvError::Empty) => {
            state.frame = state.frame.wrapping_add(1);
            LoadingAction::None
        }
        Err(TryRecvError::Disconnected) => LoadingAction::Failed(failure_message(
            &state.book.title,
            "the conversion worker ended unexpectedly",
        )),
    }
}

pub fn handle_key(_state: &mut State, key: KeyEvent) -> LoadingAction {
    match key.code {
        KeyCode::Esc => LoadingAction::Cancelled,
        _ => LoadingAction::None,
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let spinner = SPINNER[state.frame % SPINNER.len()];
    let lines = vec![
        Line::from(vec![
            Span::styled(
                spinner,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Opening "),
            Span::styled(
                state.book.title.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("…"),
        ]),
        Line::default(),
        Line::from(Span::styled(
            "Esc cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let rect = centered_rect(area.width, lines.len() as u16, area);
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), rect);
}

pub fn help_sections(_state: &State) -> Vec<HelpSection> {
    vec![HelpSection {
        title: "Loading",
        bindings: LOADING_BINDINGS,
    }]
}

fn failure_message(title: &str, detail: &str) -> String {
    format!("could not open `{title}`: {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::books::EmbedStatus;
    use crate::reader::Chapter;
    use std::sync::mpsc::{channel, Sender};

    fn sample_book() -> CatalogBook {
        CatalogBook {
            id: 1,
            title: "Dune".to_string(),
            author: Some("Herbert".to_string()),
            format: "epub".to_string(),
            file_path: "books/1/dune.epub".to_string(),
            added_at: "2024-01-01".to_string(),
            description: None,
            series_name: None,
            series_index: None,
            rating: None,
            isbn: None,
            publisher: None,
            language: None,
            published_date: None,
            tags: vec![],
            embed_status: EmbedStatus::Pending,
            embed_synced_at: None,
        }
    }

    fn state_with_channel() -> (State, Sender<Result<domain::Book, String>>) {
        let (tx, rx) = channel();
        let state = State {
            catalog_dir: PathBuf::from("/tmp/cdx-test-dummy"),
            book: sample_book(),
            settings: ReaderSettings::default(),
            rx,
            frame: 0,
        };
        (state, tx)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn advance_with_book_returns_ready() {
        let (mut state, tx) = state_with_channel();
        let book = domain::Book {
            id: 1,
            title: "Dune".into(),
            author: None,
            chapters: vec![Chapter::from_text("One".into(), "text".into())],
        };
        tx.send(Ok(book)).expect("receiver is alive in the state");
        assert!(matches!(advance(&mut state), LoadingAction::Ready(_)));
    }

    #[test]
    fn advance_with_error_returns_failed_with_title() {
        let (mut state, tx) = state_with_channel();
        tx.send(Err("boom".into()))
            .expect("receiver is alive in the state");
        match advance(&mut state) {
            LoadingAction::Failed(msg) => {
                assert!(msg.contains("Dune"));
                assert!(msg.contains("boom"));
            }
            _ => panic!("an error from the worker must surface as Failed"),
        }
    }

    #[test]
    fn advance_with_empty_channel_animates_and_waits() {
        let (mut state, _tx) = state_with_channel();
        assert!(matches!(advance(&mut state), LoadingAction::None));
        assert_eq!(state.frame, 1);
        assert!(matches!(advance(&mut state), LoadingAction::None));
        assert_eq!(state.frame, 2);
    }

    #[test]
    fn advance_with_dead_worker_returns_failed() {
        let (mut state, tx) = state_with_channel();
        drop(tx);
        assert!(matches!(advance(&mut state), LoadingAction::Failed(_)));
    }

    #[test]
    fn esc_cancels_and_other_keys_are_ignored() {
        let (mut state, _tx) = state_with_channel();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Esc)),
            LoadingAction::Cancelled
        ));
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Enter)),
            LoadingAction::None
        ));
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Char('x'))),
            LoadingAction::None
        ));
    }
}
