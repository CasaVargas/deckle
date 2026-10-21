// SPDX-License-Identifier: GPL-3.0-or-later
//! Place names to coordinates (Daily design §5c), with Open-Meteo's free geocoder. "Carthage,
//! NC" asks for Carthage and keeps the matches in North Carolina.

use serde::{Deserialize, Serialize};

use crate::fetch::{Net, encode};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    #[serde(default)]
    pub admin1: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub country_code: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

impl Place {
    /// "Carthage, NC" for US places, "Lyon, France" elsewhere.
    pub fn label(&self) -> String {
        let region = match (self.country_code.as_deref(), &self.admin1) {
            (Some("US"), Some(state)) => state_code(state)
                .map(str::to_owned)
                .unwrap_or(state.clone()),
            (_, _) => self.country.clone().unwrap_or_default(),
        };
        if region.is_empty() {
            self.name.clone()
        } else {
            format!("{}, {region}", self.name)
        }
    }
}

const STATES: [(&str, &str); 51] = [
    ("AL", "Alabama"),
    ("AK", "Alaska"),
    ("AZ", "Arizona"),
    ("AR", "Arkansas"),
    ("CA", "California"),
    ("CO", "Colorado"),
    ("CT", "Connecticut"),
    ("DE", "Delaware"),
    ("DC", "District of Columbia"),
    ("FL", "Florida"),
    ("GA", "Georgia"),
    ("HI", "Hawaii"),
    ("ID", "Idaho"),
    ("IL", "Illinois"),
    ("IN", "Indiana"),
    ("IA", "Iowa"),
    ("KS", "Kansas"),
    ("KY", "Kentucky"),
    ("LA", "Louisiana"),
    ("ME", "Maine"),
    ("MD", "Maryland"),
    ("MA", "Massachusetts"),
    ("MI", "Michigan"),
    ("MN", "Minnesota"),
    ("MS", "Mississippi"),
    ("MO", "Missouri"),
    ("MT", "Montana"),
    ("NE", "Nebraska"),
    ("NV", "Nevada"),
    ("NH", "New Hampshire"),
    ("NJ", "New Jersey"),
    ("NM", "New Mexico"),
    ("NY", "New York"),
    ("NC", "North Carolina"),
    ("ND", "North Dakota"),
    ("OH", "Ohio"),
    ("OK", "Oklahoma"),
    ("OR", "Oregon"),
    ("PA", "Pennsylvania"),
    ("RI", "Rhode Island"),
    ("SC", "South Carolina"),
    ("SD", "South Dakota"),
    ("TN", "Tennessee"),
    ("TX", "Texas"),
    ("UT", "Utah"),
    ("VT", "Vermont"),
    ("VA", "Virginia"),
    ("WA", "Washington"),
    ("WV", "West Virginia"),
    ("WI", "Wisconsin"),
    ("WY", "Wyoming"),
];

fn state_code(name: &str) -> Option<&'static str> {
    STATES
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .map(|(c, _)| *c)
}

pub fn url(name: &str) -> String {
    format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=10&language=en&format=json",
        encode(name)
    )
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    results: Vec<Place>,
}

/// Matches for `query`, best first; the part after a comma narrows them (state code or
/// name, country or country code).
pub fn parse(json: &[u8], query: &str) -> Result<Vec<Place>, String> {
    let reply: Reply = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let Some((_, qualifier)) = query.split_once(',') else {
        return Ok(reply.results);
    };
    let q = qualifier.trim().to_lowercase();
    if q.is_empty() {
        return Ok(reply.results);
    }
    let matches = |p: &Place| {
        let state = p.admin1.as_deref().unwrap_or_default();
        [
            Some(state.to_lowercase()),
            state_code(state).map(str::to_lowercase),
            p.country.as_deref().map(str::to_lowercase),
            p.country_code.as_deref().map(str::to_lowercase),
        ]
        .into_iter()
        .flatten()
        .any(|v| v == q || (q.len() > 3 && v.starts_with(&q)))
    };
    Ok(reply.results.into_iter().filter(matches).collect())
}

pub fn search(query: &str, net: &dyn Net) -> Result<Vec<Place>, String> {
    let name = query.split(',').next().unwrap_or(query).trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }
    parse(&net.get(&url(name))?, query)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/geocode-carthage.json"
        ))
        .unwrap()
    }

    #[test]
    fn a_state_code_narrows_the_matches() {
        let nc = parse(&fixture(), "Carthage, NC").unwrap();
        assert_eq!(nc.len(), 1);
        assert_eq!(nc[0].label(), "Carthage, NC");
        assert!((nc[0].latitude - 35.346).abs() < 0.01);
        assert_eq!(parse(&fixture(), "Carthage, north car").unwrap().len(), 1);
        assert_eq!(parse(&fixture(), "Carthage").unwrap().len(), 10);
        assert!(parse(&fixture(), "Carthage, Narnia").unwrap().is_empty());
    }

    #[test]
    fn the_query_sends_only_the_name() {
        assert!(url("Carthage").contains("name=Carthage&"));
        let none = |_: &str| -> Result<Vec<u8>, String> { Ok(br#"{}"#.to_vec()) };
        assert!(search("  , NC", &none).unwrap().is_empty());
        assert!(search("Nowhere", &none).unwrap().is_empty());
    }
}
