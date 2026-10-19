use color_eyre::{
    Result,
    eyre::{OptionExt, eyre},
};
use regex::regex;
use scraper::{ElementRef, Html, Selector};
use std::{fmt::Display, str::FromStr, sync::OnceLock};

use crate::{event::{Event, Session, TrackInfo, TrackSet}, inputs::EventParser, tz::TzOrOffset};

static CARD_SELECTOR: OnceLock<Selector> = OnceLock::new();
static ROOM_SELECTOR: OnceLock<Selector> = OnceLock::new();
static TIME_SELECTOR: OnceLock<Selector> = OnceLock::new();
static TITLE_SELECTOR: OnceLock<Selector> = OnceLock::new();
static DESCRIPTION_SELECTOR: OnceLock<Selector> = OnceLock::new();
static SPEAKER_SELECTOR: OnceLock<Selector> = OnceLock::new();

pub struct Sessionize;
impl Display for Sessionize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sessionize HTML page")
    }
}

impl EventParser for Sessionize {
    fn parse_from_str(&self, data: &str) -> Result<Event> {
        let fragment = Html::parse_fragment(data);
        // Check to see if it was valid HTML
        if fragment.root_element().children().next().is_none()  {
            return Err(eyre!("Not HTML"));
        }
        // Ok, so this won't actually work if the event straddles a timezone shift, because sessionize is like that
        //
        // Sorry.
        let time_zone =
            regex!(r#"sessionize\.eventTimezone\s*=\s*['"]([^'"]+)"#)
            .captures(data)
            .and_then(|capture| capture.get(1))
            .and_then(|time_zone| {
                let time_zone = time_zone.as_str();
                match TzOrOffset::from_str(time_zone) {
                    Ok(res) => Some(res),
                    Err(e) => { println!("WARNING: found but could not parse timezone {time_zone}: {e}"); None }
                }
            })
            .unwrap_or_else(|| {
                println!("WARNING: Couldn't extract Sessionize timezone, assuming everywhere is London.");
                TzOrOffset::TimeZone(chrono_tz::Europe::London)
            });

        #[expect(clippy::unwrap_used)]
        let card_selector = CARD_SELECTOR
        .get_or_init(|| Selector::parse(r#"li.sz-session"#).unwrap());

        let mut ret = Event::new(time_zone);
        for card in fragment.select(card_selector) {
            match parse_card(&mut ret, &card) {
                Ok(session) => ret.add_session(session),
                Err(err) => println!("Warn: {err}"),
            }
        }

        Ok(ret)
    }
}

fn parse_card(event: &mut Event, card: &ElementRef<'_>) -> Result<Session> {
    #[expect(clippy::unwrap_used)]
    let room_selector = ROOM_SELECTOR
        .get_or_init(|| Selector::parse(r#"div.sz-session__room"#).unwrap());
    #[expect(clippy::unwrap_used)]
    let time_selector = TIME_SELECTOR
        .get_or_init(|| Selector::parse(r#"div.sz-session__time"#).unwrap());
    #[expect(clippy::unwrap_used)]
    let title_selector = TITLE_SELECTOR
        .get_or_init(|| Selector::parse(r#"h3.sz-session__title"#).unwrap());
    #[expect(clippy::unwrap_used)]
    let description_selector = DESCRIPTION_SELECTOR.get_or_init(|| {
        Selector::parse(r#"p.sz-session__description"#).unwrap()
    });
    #[expect(clippy::unwrap_used)]
    let speaker_selector = SPEAKER_SELECTOR
        .get_or_init(|| Selector::parse(r#"ul.sz-session__speakers"#).unwrap());

    // The track is stored in the room
    let track = event.get_or_create_track(select_str(card, room_selector, "room")?, |name| TrackInfo{name: name.to_owned()});

    let time =
        card.select(time_selector)
            .next().expect("Could not find time element")
            .attr("data-sztz").expect("Oh no they got rid of the time data attribute. Good luck fixing this <3");

    let title = select_str(card, title_selector, "title")?.to_owned();
    let description = select_str(card, description_selector, "description")?.to_owned();
    let speakers = card
        .select(speaker_selector)
        .flat_map(|ele| ele.text())
        .filter(|s| s.len() > 1)
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();

    // The last two pipe delimeted values are the time in UTC
    //
    // TODO: we can actually infer timezone from this for some reason if we're willing to parse the text
    let mut parts = time.split_terminator('|');
    let end = parts.next_back().ok_or_eyre("Malformed time")?.parse()?;
    let start = parts.next_back().ok_or_eyre("Malformed time")?.parse()?;

    Ok(Session {
        title,
        description,
        speakers,
        room: None,
        start_utc: start,
        end_utc: end,
        tracks: TrackSet::from_iter([track])
    })
}

fn select_str<'a>(
    card: &'a ElementRef<'_>,
    selector: &'a Selector,
    name: &'a str,
) -> Result<&'a str> {
    Ok(card
        .select(selector)
        .next()
        .ok_or_else(|| eyre!("Card missing {name} definition"))?
        .text()
        .next()
        .ok_or_else(|| eyre!("{name} definition missing text"))?
        .trim())
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use std::io::Cursor;

//     const BELFAST: &str = include_str!("../../test-data/belfast.html");

//     #[test]
//     fn test_html() {
//         let sessions: SessionizePage = BELFAST.parse().unwrap();
//         insta::assert_yaml_snapshot!(sessions);
//     }

//     #[test]
//     fn test_show_file() {
//         let sessions: SessionizePage = BELFAST.parse().unwrap();
//         let event = crate::Event::try_from(sessions).unwrap();
//         insta::assert_yaml_snapshot!(event);

//         let mut file = Vec::<u8>::new();
//         let mut cur = Cursor::new(&mut file);

//         dbg!(&event);

//         let t1 = event.tracks.get("Track 1 - Exhibition Centre").unwrap();
//         t1.make_track(&mut cur).unwrap();

//         let out = String::from_utf8(file).unwrap();
//         assert_eq!(
//             out,
//             "\
// 09:45-10:25\tMorning Keynote

// 10:25-11:10\tWhat the Real AI Attacks of the Last
// 10:25-11:10\tTwo Years Actually Taught Us
// 10:25-11:10\t - Liana Anca Tomescu

// 11:30-12:15\t45,724 Networks, One Flipper Zero:
// 11:30-12:15\tMapping the Wireless Threat Landscape
// 11:30-12:15\tof Northern Ireland
// 11:30-12:15\t - M0r3al

// 12:15-13:00\tHacking Browsers: The Easy Way
// 12:15-13:00\t - Robbe Van Roey / PinkDraconian

// 13:45-14:30\tReverse Engineering an OAuth Supply
// 13:45-14:30\tChain Attack
// 13:45-14:30\t - art

// 14:30-15:15\tNo CVV, No 3DS, No Problem: Chaining
// 14:30-15:15\tTrust Failures Across a Live Aviation
// 14:30-15:15\tPayment Stack
// 14:30-15:15\t - Esat Berk Kandemir

// 15:40-16:25\tChaining the Unchainable: Finding and
// 15:40-16:25\tExploiting Logic Flaws in Modern Web
// 15:40-16:25\tArchitectures
// 15:40-16:25\t - Raj Dhruv

// 16:25-17:10\tLeaks on the Livewire: How a Few Blocked
// 16:25-17:10\tRequests Led to a Shocking Discovery
// 16:25-17:10\t - Daniel Johnston

// 17:10-17:50\tEvening Keynote
// 17:10-17:50\t - Aleksandra Aytova

// "
//         );
//     }
// }
