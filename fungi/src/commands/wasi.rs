use super::FungiArgs;
use fungi_wasi::{run, IpcMessage};
use ipc_channel::ipc;

pub async fn wasi(args: &FungiArgs) {
    let (server, name) = ipc::IpcOneShotServer::<IpcMessage>::new().unwrap();
    println!("{}", name);
    run(server, args.wasi_root_dir(), args.wasi_bin_dir()).await;
    println!("{} finished", name);
}
