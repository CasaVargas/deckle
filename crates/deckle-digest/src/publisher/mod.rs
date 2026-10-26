// SPDX-License-Identifier: GPL-3.0-or-later
//! Publisher subscriptions: the adapter interface (publisher subscriptions design §5–§6).
//!
//! A [`Publisher`] knows one CMS family's sign-in, entitlement and unlock rules; the
//! [`registry`] lists the known ones. [`Access`] records how a page was fetched.

use std::collections::BTreeMap;

pub mod blox;

/// The HTTP surface an adapter signs in through: cookie-carrying, per site.
pub trait SiteClient: Sync {
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;
    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String>;
}

/// What a signed-in account may read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entitlement {
    /// The account's active service ids.
    pub services: Vec<String>,
    /// Service id → display name, when the site's reply carries one.
    pub names: BTreeMap<String, String>,
}

impl Entitlement {
    /// The account's first service as a reader would name it: its display name, else its id.
    pub fn label(&self) -> Option<String> {
        let id = self.services.first()?;
        Some(self.names.get(id).cloned().unwrap_or_else(|| id.clone()))
    }
}

/// One CMS family's sign-in and unlock rules.
pub trait Publisher: Sync {
    fn kind(&self) -> &'static str;
    fn detect(&self, html: &str) -> bool;
    fn signed_in(&self, html: &str) -> bool;
    fn login(&self, site: &str, user: &str, pw: &str, c: &dyn SiteClient) -> Result<(), String>;
    fn entitlement(&self, site: &str, c: &dyn SiteClient) -> Result<Entitlement, String>;
    fn unlock(&self, html: &str, ent: &Entitlement) -> Option<String>;
    fn sections(&self) -> &[(&'static str, &'static str)];
    fn feed_url(&self, site: &str, slug: &str, n: usize) -> String;
}

/// Every known adapter.
pub fn registry() -> Vec<Box<dyn Publisher>> {
    vec![Box::new(blox::Blox)]
}

/// The `kind` of the first adapter that recognises this page, if any.
pub fn detect_kind(html: &str) -> Option<&'static str> {
    registry().iter().find(|p| p.detect(html)).map(|p| p.kind())
}

/// How a page was obtained.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Open,
    Subscription {
        site: String,
        service: String,
    },
    Walled {
        site: String,
        why: Walled,
    },
}

/// Why a subscriber page could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Walled {
    NotSignedIn,
    /// The adapter's own message; never contains the password.
    LoginFailed(String),
    NotInPlan,
    NoAdapter,
}

impl Walled {
    /// The reader-facing note for a walled story.
    pub fn note(&self, site: &str) -> String {
        match self {
            Walled::NotSignedIn => format!("Subscriber story (not signed in to {site})."),
            Walled::LoginFailed(e) => format!("Subscriber story (sign-in to {site} failed: {e})."),
            Walled::NotInPlan => format!("Subscriber story (your {site} plan doesn't include it)."),
            Walled::NoAdapter => {
                format!("Subscriber story (Deckle doesn't know how {site} signs in yet).")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Net;

    #[test]
    fn the_registry_has_blox() {
        let kinds: Vec<&str> = registry().iter().map(|p| p.kind()).collect();
        assert_eq!(kinds, vec!["blox"]);
        assert_eq!(detect_kind("<html>tncms</html>"), Some("blox"));
        assert_eq!(detect_kind("<html>wordpress</html>"), None);
    }
    #[test]
    fn an_entitlement_is_labelled_by_name_when_known() {
        let mut e = Entitlement {
            services: vec!["3".into()],
            ..Default::default()
        };
        assert_eq!(e.label().as_deref(), Some("3"));
        e.names.insert("3".into(), "Pilot Plus".into());
        assert_eq!(e.label().as_deref(), Some("Pilot Plus"));
        assert_eq!(Entitlement::default().label(), None);
    }
    #[test]
    fn walled_notes() {
        assert_eq!(
            Walled::NotSignedIn.note("thepilot.com"),
            "Subscriber story (not signed in to thepilot.com)."
        );
        assert_eq!(
            Walled::NoAdapter.note("thepilot.com"),
            "Subscriber story (Deckle doesn't know how thepilot.com signs in yet)."
        );
    }
    #[test]
    fn a_closure_net_is_open() {
        let net = |_: &str| Ok(b"<p>hi</p>".to_vec());
        let (b, a) = net.get_page_as("https://x.test/").unwrap();
        assert_eq!(a, Access::Open);
        assert_eq!(b, b"<p>hi</p>");
    }
}
