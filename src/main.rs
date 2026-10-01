/// mimalloc instead of the system allocator: linting allocates many small,
/// short-lived strings and vectors from every worker thread at once.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Entry point: delegates to the CLI crate.
fn main() -> anyhow::Result<()> {
  gale_cli::run()
}
