// log_bridge.rs

use indicatif::MultiProgress;
use std::io;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::writer::MakeWriter;

/// A Writer that suspends progress bars while writing
pub struct SuspendingWriter<W> {
    inner: W,
    mp: Arc<Mutex<MultiProgress>>,
}

impl<W: io::Write> io::Write for SuspendingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Ok(mp) = self.mp.lock() {
            // Suspend progress bars while writing
            mp.suspend(|| self.inner.write(buf))
        } else {
            // If we can't lock the MultiProgress, just write directly
            self.inner.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A MakeWriter implementation that wraps another MakeWriter with suspending capability
pub struct MakeSuspendingWriter<W> {
    inner: W,
    mp: Arc<Mutex<MultiProgress>>,
}

impl<W> MakeSuspendingWriter<W> {
    pub fn new(inner: W, mp: MultiProgress) -> Self {
        Self {
            inner,
            mp: Arc::new(Mutex::new(mp)),
        }
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
            mp: Arc::clone(&self.mp),
        }
    }
}

/// Wraps a MultiProgress for integration with tracing
pub struct TracingWrapper {
    mp: MultiProgress,
}

impl TracingWrapper {
    pub fn new(mp: MultiProgress) -> Self {
        Self { mp }
    }

    /// Creates a writer that suspends progress bars while writing log output
    pub fn layer(self) -> impl for<'a> tracing_subscriber::Layer<tracing_subscriber::Registry> {
        let make_writer = MakeSuspendingWriter::new(io::stdout, self.mp);

        tracing_subscriber::fmt::layer()
            .with_writer(make_writer)
            .with_ansi(true)
            .with_file(true)
            .with_line_number(true)
    }
}
