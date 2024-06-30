use super::FungiArgs;
use fungi_wasi::run;

pub async fn wasi(args: &FungiArgs) {
    let fungi_dir = args.fungi_dir();
    let ipc_server_name = run(fungi_dir);
    println!("{}", ipc_server_name);
    std::thread::park();
}