fn main() {
    match gmod_api_update::run_from_env() {
        Ok(summary) => {
            println!(
                "generated {} document pages, {} entries, {} hooks, {} classes from {} official page(s)",
                summary.document_pages,
                summary.entries,
                summary.hooks,
                summary.classes,
                summary.official_pages
            );
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}
