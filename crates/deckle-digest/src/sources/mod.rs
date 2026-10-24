// SPDX-License-Identifier: GPL-3.0-or-later
//! One module per network source; each turns a response into a [`crate::Section`].

pub mod feeds;
pub mod lottery;
pub mod onthisday;
pub mod reading;
pub mod weather;

/// The note shown in a section whose source failed.
pub(crate) fn unavailable(title: &str, why: impl std::fmt::Display) -> crate::Section {
    crate::Section {
        brief: Vec::new(),
        key: String::new(),
        title: title.into(),
        unavailable: Some(format!("Unavailable today ({why}).")),
        ..Default::default()
    }
}
