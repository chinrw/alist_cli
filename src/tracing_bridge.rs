// log_bridge.rs

use std::io;

use indicatif::MultiProgress;
use tracing_subscriber::fmt::writer::MakeWriter;

/// A Writer that suspends progress bars while writing
pub struct SuspendingWriter<W> {
    inner: W,
    mp: MultiProgress,
}

impl<W: io::Write> io::Write for SuspendingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Suspend progress bars while writing
        self.mp.suspend(|| self.inner.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A MakeWriter implementation that wraps another MakeWriter with suspending
/// capability
pub struct MakeSuspendingWriter<W> {
    inner: W,
    mp: MultiProgress,
}

impl<W> MakeSuspendingWriter<W> {
    pub fn new(inner: W, mp: MultiProgress) -> Self {
        Self { inner, mp }
    }
}

impl<'a, W> MakeWriter<'a> for MakeSuspendingWriter<W>
where
    W: MakeWriter<'a>,
{
    type Writer = SuspendingWriter<W::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        SuspendingWriter {
            inner: self.inner.make_writer(),
            mp: self.mp.clone(),
        }
    }
}
