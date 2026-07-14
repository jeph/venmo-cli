use std::io::{self, Write};

use super::{ReadCall, Transcript};

#[derive(Clone, Copy)]
pub(super) enum Stream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WriterState {
    pub(super) text: String,
    pub(super) flush_count: u32,
    pub(super) fail_write: bool,
}

pub(super) struct RecordingWriter {
    stream: Stream,
    pub(super) state: WriterState,
    transcript: Transcript,
    recorded_write: bool,
}

impl RecordingWriter {
    pub(super) fn new(stream: Stream, state: WriterState, transcript: Transcript) -> Self {
        Self {
            stream,
            state,
            transcript,
            recorded_write: false,
        }
    }

    fn record_write_once(&mut self) {
        if !self.recorded_write {
            self.recorded_write = true;
            self.transcript.borrow_mut().push(match self.stream {
                Stream::Stdout => ReadCall::StdoutWrite,
                Stream::Stderr => ReadCall::StderrWrite,
            });
        }
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.record_write_once();
        if self.state.fail_write {
            return Err(io::Error::other("synthetic output failure"));
        }
        self.state
            .text
            .push_str(std::str::from_utf8(buffer).map_err(io::Error::other)?);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.transcript.borrow_mut().push(match self.stream {
            Stream::Stdout => ReadCall::StdoutFlush,
            Stream::Stderr => ReadCall::StderrFlush,
        });
        self.state.flush_count += 1;
        Ok(())
    }
}

pub(super) fn writer(stream: Stream, transcript: Transcript) -> RecordingWriter {
    RecordingWriter::new(stream, WriterState::default(), transcript)
}

pub(super) fn writer_state(text: &str) -> WriterState {
    WriterState {
        text: text.to_owned(),
        ..WriterState::default()
    }
}
