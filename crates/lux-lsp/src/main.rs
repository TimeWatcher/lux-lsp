mod server;

fn main() {
    if let Err(err) = server::run() {
        eprintln!("lux-lsp: {err}");
        std::process::exit(1);
    }
}
