use crate::stdio_impl::StdioImpl;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use wasmtime::{Config, Engine, Linker, Module, Store, TypedFunc};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::WasiCtxBuilder;

pub struct WasiRuntime {
    engine: Engine,

    root_dir: PathBuf,
    bin_dir: PathBuf,
}

pub struct WasiCommand {
    func: TypedFunc<(), ()>,
    store: Store<WasiP1Ctx>,
}

impl WasiCommand {
    pub async fn run(mut self) -> Result<()> {
        self.func
            .call_async(&mut self.store, ())
            .await
            .context("failed to call function")?;
        tokio::time::sleep(std::time::Duration::from_micros(10)).await; // TODO wait io
        Ok(())
    }
}

impl WasiRuntime {
    pub fn new(root_dir: PathBuf, bin_dir: PathBuf) -> Result<Self> {
        let mut config = Config::new();
        config.async_support(true);
        // TODO load from file
        config.cache_config_load_default().unwrap();
        let engine = Engine::new(&config).context("failed to create engine")?;

        Ok(Self {
            engine,
            root_dir,
            bin_dir,
        })
    }

    pub async fn command(&mut self, args: Vec<String>, stdio: StdioImpl) -> Result<WasiCommand> {
        let bin = &args[0];
        // find bin in bin_dir
        let bin_path = self.bin_dir.join(bin);
        if !bin_path.exists() {
            bail!("`{}` not found", bin);
        }

        let wasi_ctx = WasiCtxBuilder::new()
            .stdin(stdio.stdin) // TODO
            .stdout(stdio.stdout)
            .stderr(stdio.stderr)
            .args(&args)
            .build_p1();

        let mut store: Store<WasiP1Ctx> = Store::new(&self.engine, wasi_ctx);

        let module =
            Module::from_file(&self.engine, bin_path).context("failed to create module")?;

        let mut linker: Linker<WasiP1Ctx> = Linker::new(&self.engine);
        preview1::add_to_linker_async(&mut linker, |t| t).context("failed to add linker")?;

        let func = linker
            .module_async(&mut store, "", &module)
            .await
            .context("failed to link module")?
            .get_default(&mut store, "")
            .context("failed to get default function")?
            .typed::<(), ()>(&store)
            .context("failed to get typed function")?;

        Ok(WasiCommand { func, store })
    }
}
