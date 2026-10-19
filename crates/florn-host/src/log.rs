//! The diagnostic log (RFC 007 rule 12; SEC-008): every notification of severity `info` and above,
//! with the wall-clock time as metadata and the run's identifier; the canonical encoding, and one
//! JSON object per line on export. Never a sentence.

use florn_control::Notification;
use florn_control::notification::json_str;
use florn_model::canonical::Encoder;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Metadata (RFC 001 rule 42): milliseconds since the Unix epoch, from the platform.
    pub wall_ms: u64,
    pub run_id: String,
    pub notification: Notification,
}

#[derive(Clone, Debug, Default)]
pub struct DiagnosticLog {
    entries: Vec<Entry>,
}

impl DiagnosticLog {
    pub fn push(&mut self, wall_ms: u64, run_id: &str, notification: Notification) {
        self.entries.push(Entry {
            wall_ms,
            run_id: run_id.to_string(),
            notification,
        });
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn codes(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|e| e.notification.code.as_str())
            .collect()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The canonical encoding: a list of entries, each the wall time, the run id, the notification.
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::default();
        e.u32(self.entries.len() as u32);
        for en in &self.entries {
            e.u64(en.wall_ms);
            e.text(&en.run_id);
            e.bytes(&en.notification.encode());
        }
        e.finish()
    }

    /// One JSON object per line, ending with a newline.
    pub fn to_jsonl(&self) -> String {
        let mut s = String::new();
        for en in &self.entries {
            s.push_str(&format!(
                "{{\"wall-ms\":{},\"run\":{},\"notification\":{}}}\n",
                en.wall_ms,
                json_str(&en.run_id),
                en.notification.to_json()
            ));
        }
        s
    }
}
