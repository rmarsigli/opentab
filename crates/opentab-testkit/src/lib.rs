pub mod archives;
pub mod banks;
mod build;
mod effects;
mod fill;
pub mod http;
mod seed;
mod text;

use opentab_model::Score;
use proptest::prelude::Strategy;

pub use fill::min_beats_for_bar;
pub use seed::ScoreGenConfig;
pub use text::arb_any_text;

pub fn arb_score() -> impl Strategy<Value = Score> {
    arb_score_with(ScoreGenConfig::default())
}

pub fn arb_score_with(cfg: ScoreGenConfig) -> impl Strategy<Value = Score> {
    seed::arb_score_seed(cfg).prop_map(move |seed| build::build_score(seed, cfg))
}
