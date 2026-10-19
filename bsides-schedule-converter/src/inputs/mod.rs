use std::{fmt::Display, io::Read};

use color_eyre::{Result, eyre::eyre};
use regex::Regex;

pub mod pretalx;
pub mod sessionize;
// pub mod sessionize_json;

use pretalx::Pretalx;
use sessionize::Sessionize;
// use sessionize_json::SessionizeJson;

use crate::event::Event;

pub trait EventParser: Display {
    fn parse_from_str(&self, data: &str) -> Result<Event>;
}
impl<'a> AsRef<dyn EventParser + 'a> for &'a dyn EventParser  {
    fn as_ref(&self) -> &(dyn EventParser + 'a) {
        *self
    }
}

pub const DEFAULT_PARSERS: &[&dyn EventParser] = &[
    &Pretalx,
    &Sessionize,
];

impl Event {
    pub fn parse_from_str(data: &str, registry: impl IntoIterator<Item=impl AsRef<dyn EventParser>>) -> Option<Event> {
        for i in registry {
            let i = i.as_ref();
            match i.parse_from_str(data) {
                Ok(res) => {
                    println!("Parsed format {i}");
                    return Some(res)
                }
                Err(e) => {
                    println!("Rejected format {i}: {e}")
                }
            }
        }
        None
    }
    pub fn parse(location: &str, registry: impl IntoIterator<Item=impl AsRef<dyn EventParser>>) -> Result<Event> {
        // Work out where to get the data from, and load it
        let url_regex = Regex::new("(?i)(https?|ftp)://").expect("Could not compile http regex");
        let mut data: String;
        if url_regex.is_match_at(location, 0) {
            data = String::new();
            reqwest::blocking::get(location)?.read_to_string(&mut data)?;
            println!("Downloaded {} bytes from {location}", data.len());
        }
        else {
            data = std::fs::read_to_string(location)?;
            println!("Read {} bytes from {location}", data.len());
        }

        Self::parse_from_str(&data, registry).ok_or_else(|| eyre!("Could not find a parser for {location}"))
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    const BELFAST: &str = include_str!("../../test-data/sessionize/belfast.html");
    #[test]
    fn test_sessionize() {
        let format = Event::parse_from_str(BELFAST, DEFAULT_PARSERS).unwrap();
        insta::assert_yaml_snapshot!(format);
    }

    const BRISTOL: &str = include_str!("../../test-data/pretalx/bristol-2026.json");

    #[test]
    fn test_pretalx() {
        let format = Event::parse_from_str(BRISTOL, DEFAULT_PARSERS).unwrap();
        insta::assert_yaml_snapshot!(format);
    }


}
