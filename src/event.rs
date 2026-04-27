use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    cancel: CancellationToken,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let token = cancel.clone();

        tokio::spawn(async move {
            loop {
                if token.is_cancelled() {
                    break;
                }
                let has_event = match event::poll(tick_rate) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("event poll error: {}", e);
                        continue;
                    }
                };
                if has_event {
                    match event::read() {
                        Ok(CrosstermEvent::Key(key)) => {
                            if key.kind == KeyEventKind::Press
                                && tx.send(Event::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(CrosstermEvent::Resize(w, h)) => {
                            let _ = tx.send(Event::Resize(w, h));
                        }
                        Err(e) => {
                            log::warn!("event read error: {}", e);
                        }
                        _ => {}
                    }
                } else {
                    let _ = tx.send(Event::Tick);
                }
            }
        });

        Self { rx, cancel }
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }
}
