//! Offline representative workload for instrumentation PGO.
//! Usage: pgo_harness [modules_dir] [iterations]

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{home}/Downloads/for mac espada")
        });
    let iters: usize = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    eprintln!("PGO harness · modules={dir} · iters={iters}");
    for i in 1..=iters {
        match espada3_7_lib::pgo_smoke(&dir) {
            Ok(msg) => eprintln!("  [{i}/{iters}] {msg}"),
            Err(e) => {
                eprintln!("  [{i}/{iters}] ERROR: {e}");
                std::process::exit(1);
            }
        }
    }
    eprintln!("PGO harness done.");
}
