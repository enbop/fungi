use anyhow::Result;
use clap::Parser;
use fungi_wasi::WasiRuntime;

#[derive(Debug, Clone, Default, Parser)]
pub struct WasiArgs {
    #[clap(help = "Name of the WebAssembly module")]
    pub wasm_module: String,
}

pub async fn run(args: WasiArgs) -> Result<()> {
    println!("Running WASI module: {}", args.wasm_module);
    let mut rt = WasiRuntime::new("".into(), "".into())?;
    rt.run(vec![args.wasm_module]).await?;
    Ok(())
}
