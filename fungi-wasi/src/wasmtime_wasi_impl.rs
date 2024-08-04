use anyhow::{bail, Context, Result};
use bytes::Bytes;
use std::path::PathBuf;
use tokio::sync::mpsc;
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{
    HostOutputStream, StdoutStream, StreamError, StreamResult, Subscribe, WasiCtxBuilder,
};

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

pub struct WasiRuntime {
    engine: Engine,
    linker: Linker<WasiP1Ctx>,

    root_dir: PathBuf,
    bin_dir: PathBuf,
}

impl WasiRuntime {
    pub fn new(root_dir: PathBuf, bin_dir: PathBuf) -> Result<Self> {
        let mut config = Config::new();
        config.async_support(true);
        let engine = Engine::new(&config).context("failed to create engine")?;

        let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
        preview1::add_to_linker_async(&mut linker, |t| t).context("failed to add linker")?;

        Ok(Self {
            engine,
            linker,
            root_dir,
            bin_dir,
        })
    }

    pub async fn spawn(&mut self, args: Vec<&str>) -> Result<()> {
        let bin = args[0];
        // find bin in bin_dir
        let bin_path = self.bin_dir.join(bin);
        if !bin_path.exists() {
            bail!("failed to run cmd, {:?} not found", bin_path);
        }

        let (stdout, mut stdout_rx) = StdoutChannel::new();

        tokio::spawn(async move {
            loop {
                let Some(out) = stdout_rx.recv().await else {
                    break;
                };
                println!("CUSTOM STDOUT: {}", String::from_utf8_lossy(&out));
            }
        });

        let wasi_ctx = WasiCtxBuilder::new().stdout(stdout).args(&args).build_p1();

        let mut store = Store::new(&self.engine, wasi_ctx);

        let module =
            Module::from_file(&self.engine, bin_path).context("failed to create module")?;
        let func = self
            .linker
            .module_async(&mut store, "", &module)
            .await
            .context("failed to link module")?
            .get_default(&mut store, "")
            .context("failed to get default function")?
            .typed::<(), ()>(&store)
            .context("failed to get typed function")?;

        // Invoke the WASI program default function.
        func.call_async(&mut store, ())
            .await
            .context("failed to call function")?;
        Ok(())
    }
}
