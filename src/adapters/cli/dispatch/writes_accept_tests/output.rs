use std::io::{self, Write};

use super::{AcceptCall, Transcript};

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
    pub(super) fail_flush: bool,
}

pub(super) struct OrderedWriter {
    stream: Stream,
    pub(super) state: WriterState,
    transcript: Transcript,
    recorded_write: bool,
}

impl OrderedWriter {
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
                Stream::Stdout => AcceptCall::StdoutWrite,
                Stream::Stderr => AcceptCall::StderrWrite,
            });
        }
    }
}

impl Write for OrderedWriter {
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
            Stream::Stdout => AcceptCall::StdoutFlush,
            Stream::Stderr => AcceptCall::StderrFlush,
        });
        self.state.flush_count += 1;
        if self.state.fail_flush {
            Err(io::Error::other("synthetic flush failure"))
        } else {
            Ok(())
        }
    }
}
