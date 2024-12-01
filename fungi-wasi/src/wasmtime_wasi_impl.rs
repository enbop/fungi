use crate::stdio_impl::StdioImpl;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use wasmtime::component::TypedFunc;
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

pub struct State {
    wasi_ctx: WasiCtx,
    wasi_table: ResourceTable,
}

impl WasiView for State {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.wasi_table
    }

    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
}

pub struct WasiRuntime {
    engine: Engine,

    root_dir: PathBuf,
    bin_dir: PathBuf,
}

pub struct WasiCommand {
    func: TypedFunc<(), ()>,
    store: Store<State>,
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
        #[cfg(not(target_os = "android"))]
        config.cache_config_load_default().unwrap();
        let engine = Engine::new(&config).context("failed to create engine")?;

        Ok(Self {
            engine,
            root_dir,
            bin_dir,
        })
    }

    pub async fn run(&mut self, args: Vec<String>, stdio: Option<StdioImpl>) -> Result<()> {
        let bin = &args[0];
        // find bin in bin_dir
        let bin_path = self.bin_dir.join(bin);
        if !bin_path.exists() {
            bail!("`{}` not found", bin);
        }

        let mut wasi_ctx_builder = WasiCtxBuilder::new();
        match stdio {
            Some(stdio) => wasi_ctx_builder
                .stdin(stdio.stdin)
                .stdout(stdio.stdout)
                .stderr(stdio.stderr),
            None => wasi_ctx_builder
                .stdin(wasmtime_wasi::stdin())
                .stdout(wasmtime_wasi::stdout())
                .stderr(wasmtime_wasi::stderr()),
        };

        // TODO set permissions
        let wasi_ctx = wasi_ctx_builder
            .args(&args)
            .inherit_network()
            .allow_ip_name_lookup(true)
            .allow_tcp(true)
            .allow_udp(true)
            .build();

        let state = State {
            wasi_ctx,
            wasi_table: ResourceTable::new(),
        };
        let mut store: Store<State> = Store::new(&self.engine, state);

        let component_file = wasmtime::component::Component::from_file(&self.engine, bin_path)
            .context("failed to load module")?;
        let mut linker = wasmtime::component::Linker::new(&self.engine);
        wasmtime_wasi::add_to_linker_async(&mut linker).unwrap();

        let res = wasmtime_wasi::bindings::Command::instantiate_async(
            &mut store,
            &component_file,
            &linker,
        )
        .await?
        .wasi_cli_run()
        .call_run(&mut store)
        .await?;
        if let Err(_) = res {
            bail!("failed to call run");
        }
        Ok(())
    }
}
