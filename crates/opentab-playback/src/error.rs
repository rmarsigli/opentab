use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PlaybackError {
    #[error("a time or tempo value does not fit exact 64-bit arithmetic")]
    TimeOverflow,
    #[error("speed {percent} % is outside 25..=200 %")]
    InvalidSpeed { percent: u16 },
    #[error("a ramp grid or time step must be positive")]
    NonPositiveStep,
    #[error("a time before the start of playback has no position")]
    NegativeTime,
    #[error("the play order names a bar the score does not have")]
    OrderMismatch,
}
