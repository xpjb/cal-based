#[cfg(any(test, target_os = "windows"))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod text_edit;

pub mod model;

#[cfg(target_os = "windows")]
pub(crate) use model::*;

#[cfg(target_os = "windows")]
#[path = "widgets/lib.rs"]
pub mod widgets;
#[cfg(target_os = "windows")]
pub(crate) use widgets::*;
