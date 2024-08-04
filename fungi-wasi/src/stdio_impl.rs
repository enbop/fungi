use std::sync::{atomic::AtomicBool, Arc, Mutex};
use bytes::{BufMut, Bytes, BytesMut};
use tokio::sync::mpsc;
use wasmtime_wasi::{
    HostInputStream, HostOutputStream, StdinStream, StdoutStream, StreamError, StreamResult,
    Subscribe,
};

pub fn create_stdio() -> (StdioImpl, StdioHandle) {
    let stdin = StdinChannel::new();
    let (stdout, stdout_rx) = StdoutChannel::new();
    let (stderr, stderr_rx) = StdoutChannel::new();
    let handle = StdioHandle {
        stdin: stdin.clone(),
        stdout_rx,
        stderr_rx,
    };
    let stdio = StdioImpl {
        stdin,
        stdout,
        stderr,
    };
    (stdio, handle)
}

pub struct StdioImpl {
    pub stdin: StdinChannel,
    pub stdout: StdoutChannel,
    pub stderr: StdoutChannel,
}

pub struct StdioHandle {
    pub stdin: StdinChannel,
    pub stdout_rx: mpsc::UnboundedReceiver<Bytes>,
    pub stderr_rx: mpsc::UnboundedReceiver<Bytes>,
}

#[derive(Clone)]
pub struct StdinChannel {
    buf: Arc<Mutex<BytesMut>>,
    max_len: usize,
    closed: Arc<AtomicBool>,
}

impl StdinChannel {
    pub fn new() -> Self {
        let max_len = 1024 * 10;

        Self {
            buf: Arc::new(Mutex::new(BytesMut::with_capacity(max_len))),
            max_len,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn push_data(&self, data: Bytes) -> bool {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return false;
        }

        let mut buf = self.buf.lock().unwrap();
        if buf.len() + data.len() > self.max_len {
            return false;
        }
        buf.put(data);
        true
    }

    pub fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        self.buf.lock().unwrap().clear();
    }
}

// TODO wait io
// impl Drop for StdinChannel {
//     fn drop(&mut self) {
//         println!("Dropping StdinChannel");
//     }
// }

#[async_trait::async_trait]
impl Subscribe for StdinChannel {
    async fn ready(&mut self) {}
}

impl HostInputStream for StdinChannel {
    fn read(&mut self, size: usize) -> StreamResult<Bytes> {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StreamError::Closed);
        }
        let mut buf = self.buf.lock().unwrap();
        let len = buf.len().min(size);
        let read = buf.split_to(len);
        Ok(read.freeze())
    }
}

impl StdinStream for StdinChannel {
    fn stream(&self) -> Box<dyn HostInputStream> {
        Box::new(self.clone())
    }

    fn isatty(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct StdoutChannel {
    tx: mpsc::UnboundedSender<Bytes>,
}

impl StdoutChannel {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Bytes>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

#[async_trait::async_trait]
impl Subscribe for StdoutChannel {
    async fn ready(&mut self) {}
}

impl HostOutputStream for StdoutChannel {
    fn write(&mut self, bytes: Bytes) -> StreamResult<()> {
        self.tx.send(bytes).map_err(|e| StreamError::Trap(e.into()))
    }

    fn flush(&mut self) -> StreamResult<()> {
        // This stream is always flushed
        Ok(())
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        // This stream is always ready for writing.
        Ok(usize::MAX)
    }
}

impl StdoutStream for StdoutChannel {
    fn stream(&self) -> Box<dyn HostOutputStream> {
        Box::new(self.clone())
    }

    fn isatty(&self) -> bool {
        false
    }
}
