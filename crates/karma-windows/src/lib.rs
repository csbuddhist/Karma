#![deny(unsafe_op_in_unsafe_fn)]

mod attribution;

pub use attribution::{
    AttributedWindow, AttributionResult, Rect, SourceAttributor, UnreliableReason, WindowCandidate,
};
