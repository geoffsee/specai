//! Widget system for building UI components

pub mod builtin;
mod focus;
mod traits;

pub use focus::{FocusDirection, FocusId, FocusManager};
pub use traits::{StatefulWidget, Widget};
