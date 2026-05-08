//! BDD test binary for atlas-core.
//!
//! Run with: `cargo test -p atlas-scenarios --test atlas_bdd`

use atlas_scenarios::world::AtlasWorld;
use cucumber::World as _;

#[tokio::main]
async fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/features/atlas-core");
    // `run_and_exit` exits with non-zero status if any step failed; `run`
    // resolves regardless of step outcome and would let failures slip
    // through CI as exit code 0.
    AtlasWorld::cucumber().run_and_exit(features).await;
}
