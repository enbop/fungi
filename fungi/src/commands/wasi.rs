use super::FungiArgs;
use fungi_wasi::WasiProcess;

pub async fn wasi(args: &FungiArgs) {
    let mut process =
        WasiProcess::new(args.ipc_dir(), args.wasi_root_dir(), args.wasi_bin_dir()).unwrap();
    let ipc_sock_name = process.ipc_sock_name().to_owned();
    println!("{}", ipc_sock_name);
    process.start_listen().await.unwrap();
    println!("{} finished", ipc_sock_name);
}
