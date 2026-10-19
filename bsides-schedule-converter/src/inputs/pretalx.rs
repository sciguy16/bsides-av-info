use std::fmt::Display;

use chrono::{NaiveDate, NaiveTime, TimeDelta, Utc};
use color_eyre::{Result, eyre::OptionExt};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{event::{Event, TrackInfo, TrackSet}, inputs::EventParser};

pub struct Pretalx;
impl Display for Pretalx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pretalx")
    }
}
impl EventParser for Pretalx {
    fn parse_from_str(&self, data: &str) -> Result<Event> {
        // Parse the json
        let PretalxJson { schedule: Schedule { mut conference } } = serde_json::from_str(data)?;
        let time_zone = conference.time_zone_name.parse()?;

        // Pump it
        let mut ret = Event::new(time_zone);
        for track in &conference.tracks {
            let track_id = ret.get_or_create_track(&track.name, |name| TrackInfo { name: name.clone() });
            for day in &mut conference.days {
                let Some(cur_track) = day.rooms.swap_remove(&track.name)
                else {
                    println!(
                        "Missing key for day {}: {}",
                        day.index, track.name
                    );
                    continue;
                };
                for mut talk in cur_track {
                    let speakers = talk.persons
                        .into_iter()
                        .map(|person| {
                            if !person.public_name.is_empty() {
                                person.public_name
                            }
                            else {
                                println!(
                                    "Warning: no public name specified, falling back to \
                                    regular name for `{}`",
                                    person.name,
                                );
                                person.name
                            }
                        })
                        .collect();


                    let start =
                        day.date
                            .and_time(talk.start)
                            .and_local_timezone(ret.time_zone)
                            .single().expect("Got an ambiguous start time")
                            .with_timezone(&Utc);
                    let (hours, minutes) = talk
                        .duration
                        .split_once(':')
                        .ok_or_eyre("Malformed duration")?;
                    let hours = hours.parse::<i64>()?;
                    let minutes = minutes.parse::<i64>()?;
                    let duration = TimeDelta::minutes(hours * 60 + minutes);

                    // Deal with blank descriptions
                    if talk.r#abstract == "-" {
                        talk.r#abstract.clear();
                    }

                    ret.add_session(crate::event::Session {
                        title: talk.title,
                        description: talk.r#abstract,
                        room: None,
                        speakers,
                        start_utc: start,
                        end_utc: start + duration,
                        tracks: TrackSet::from_iter([track_id]),
                    });
                }
            }
        }

        Ok(ret)
    }
}


#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PretalxJson {
    pub schedule: Schedule,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Schedule {
    pub conference: Conference,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Conference {
    pub title: String,
    pub tracks: Vec<Track>,
    pub days: Vec<Day>,
    pub time_zone_name: String
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Track {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Day {
    pub index: i64,
    pub rooms: IndexMap<String, Vec<Session>>,
    pub date: NaiveDate
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Session {
    pub title: String,
    pub r#abstract: String,
    pub description: String,
    pub start: NaiveTime,
    pub duration: String,
    #[serde(default)]
    pub persons: Vec<Person>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Person {
    pub name: String,
    pub public_name: String,
}
