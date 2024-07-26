use std::path::PathBuf;

use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::WasiCtxBuilder;

pub struct WasiRuntime {
    engine: Engine,
    linker: Linker<WasiP1Ctx>,

    root_dir: PathBuf,
    bin_dir: PathBuf,
}

impl WasiRuntime {
    pub fn new(root_dir: PathBuf, bin_dir: PathBuf) -> Self {
        let mut config = Config::new();
        config.async_support(true);
        let engine = Engine::new(&config).unwrap();

        let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
        preview1::add_to_linker_async(&mut linker, |t| t).unwrap();

        Self {
            engine,
            linker,
            root_dir,
            bin_dir,
        }
    }

    pub async fn spawn(&mut self, args: Vec<&str>) {
        let bin = args[0];
        // find bin in bin_dir
        let bin_path = self.bin_dir.join(bin);
        if !bin_path.exists() {
            println!("{:?} not found", bin_path);
            return;
        }
        let wasi_ctx = WasiCtxBuilder::new().inherit_stdio().args(&args).build_p1();

        let mut store = Store::new(&self.engine, wasi_ctx);

        let module = Module::from_file(&self.engine, bin_path).unwrap();
        let func = self
            .linker
            .module_async(&mut store, "", &module)
            .await
            .unwrap()
            .get_default(&mut store, "")
            .unwrap()
            .typed::<(), ()>(&store)
            .unwrap();

        // Invoke the WASI program default function.
        func.call_async(&mut store, ()).await.unwrap();
    }
}
