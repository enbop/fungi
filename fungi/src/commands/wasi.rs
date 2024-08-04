use super::FungiArgs;
use fungi_wasi::WasiProcess;

pub async fn wasi(args: &FungiArgs) {
    let mut process =
        WasiProcess::new(args.ipc_dir(), args.wasi_root_dir(), args.wasi_bin_dir()).unwrap();
    let ipc_sock_name = process.ipc_sock_name().to_owned();
    println!("{}", ipc_sock_name);
    if let Err(e) = process.start_listen().await {
        eprintln!("Error: {}", e);
    }; // TODO handle error
    println!("{} finished", ipc_sock_name);
}
