fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--version") {
        println!("titan-agentd {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    match titan_agentd::config::Config::from_env_and_args(args) {
        Ok(config) => {
            if let Err(error) = titan_agentd::server::run(config) {
                eprintln!("titan-agentd: {error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("titan-agentd: {error}");
            std::process::exit(2);
        }
    }
}
