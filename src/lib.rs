mod model;

#[path = "widgets/lib.rs"]
mod widgets;

pub use widgets::run;

use model::*;
use widgets::*;
