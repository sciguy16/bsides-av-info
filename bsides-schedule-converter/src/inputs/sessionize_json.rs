use chrono::{DateTime, NaiveTime, Utc};
use color_eyre::{Result, eyre::OptionExt};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

impl TryFrom<SessionizeJson> for crate::Event {
    type Error = color_eyre::Report;
    fn try_from(this: SessionizeJson) -> Result<Self> {
        let mut tracks = IndexMap::<_, crate::show_file::ShowFile>::new();

        let rooms = this
            .rooms
            .into_iter()
            .map(|room| (room.id, room.name))
            .collect::<IndexMap<_, _>>();

        let speakers = this
            .speakers
            .into_iter()
            .map(|speaker| (speaker.id, speaker.full_name))
            .collect::<IndexMap<_, _>>();

        for session in this.sessions {
            let session_room =
                rooms.get(&session.room_id).ok_or_eyre("Missing room")?;
            let session_speakers = session
                .speakers
                .iter()
                .map(|speaker| {
                    speakers.get(speaker).ok_or_eyre("Missing speaker").cloned()
                })
                .collect::<Result<Vec<_>>>()?;
            let entry = tracks.entry(session_room.clone()).or_default();
            entry.sessions.push(crate::show_file::Session {
                title: session.title,
                description: session.description.unwrap_or_default(),
                speakers: session_speakers,
                is_break: session.is_service_session,
                start: get_time(session.starts_at),
                end: get_time(session.ends_at),
            });
        }

        Ok(Self { tracks })
    }
}

fn get_time(dt: DateTime<Utc>) -> NaiveTime {
    use chrono_tz::Europe::London;
    dt.with_timezone(&London).time()
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionizeJson {
    sessions: Vec<Session>,
    speakers: Vec<Speaker>,
    categories: Vec<Category>,
    rooms: Vec<Room>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    id: String,
    title: String,
    description: Option<String>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    is_service_session: bool,
    is_plenum_session: bool,
    speakers: Vec<String>,
    category_items: Vec<i64>,
    question_answers: Vec<()>,
    room_id: i64,
    status: Option<String>,
    is_informed: bool,
    is_confirmed: bool,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Speaker {
    id: String,
    first_name: String,
    last_name: String,
    bio: String,
    tag_line: String,
    profile_picture: String,
    is_top_speaker: bool,
    sessions: Vec<i64>,
    full_name: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    id: i64,
    title: String,
    items: Vec<CategoryItem>,
    sort: i64,
    r#type: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryItem {
    id: i64,
    name: String,
    sort: i64,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    id: i64,
    name: String,
    sort: i64,
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Cursor;

    const BELFAST_JSON: &str = include_str!("../../test-data/belfast.json");

    #[test]
    fn test_parse() {
        let _ = serde_json::from_str::<SessionizeJson>(BELFAST_JSON).unwrap();
    }

    #[test]
    fn test_session() {
        const JSON: &str = r#"
    {
      "id": "1291414",
      "title": "Morning Keynote",
      "description": "The morning keynote session",
      "startsAt": "2026-09-10T08:45:00Z",
      "endsAt": "2026-09-10T09:25:00Z",
      "isServiceSession": false,
      "isPlenumSession": true,
      "speakers": [],
      "categoryItems": [],
      "questionAnswers": [],
      "roomId": 76855,
      "liveUrl": null,
      "recordingUrl": null,
      "status": "Accepted",
      "isInformed": true,
      "isConfirmed": false
    }
"#;
        let parsed = serde_json::from_str::<Session>(JSON).unwrap();
        insta::assert_yaml_snapshot!(parsed);
    }

    #[test]
    fn test_speaker() {
        const JSON: &str = r#"
    {
      "id": "902f3fb5-ae5d-4e22-aa02-ee8c2361838e",
      "firstName": "",
      "lastName": "",
      "bio": "Adams has worked as an engineering lead in the...",
      "tagLine": "Principal Engineer, AI @ Workday",
      "profilePicture": "https://cdn.sessionize.com/image/--.png",
      "isTopSpeaker": false,
      "links": [],
      "sessions": [
        1258496
      ],
      "fullName": "Adam",
      "categoryItems": [],
      "questionAnswers": []
    }
"#;
        let parsed = serde_json::from_str::<Speaker>(JSON).unwrap();
        insta::assert_yaml_snapshot!(parsed);
    }

    #[test]
    fn test_category() {
        const JSON: &str = r#"
    {
      "id": 117963,
      "title": "Session format",
      "items": [
        {
          "id": 425309,
          "name": "Lightning talk",
          "sort": 1
        },
        {
          "id": 425310,
          "name": "Session",
          "sort": 2
        },
        {
          "id": 425311,
          "name": "Workshop",
          "sort": 3
        }
      ],
      "sort": 0,
      "type": "session"
    }
"#;
        let parsed = serde_json::from_str::<Category>(JSON).unwrap();
        insta::assert_yaml_snapshot!(parsed);
    }

    #[test]
    fn test_roomy() {
        const JSON: &str = r#"
    {
      "id": 76855,
      "name": "Track 1 - Exhibition Centre",
      "sort": 0
    }
"#;
        let parsed = serde_json::from_str::<Room>(JSON).unwrap();
        insta::assert_yaml_snapshot!(parsed);
    }

    #[test]
    fn test_show_file() {
        let sessions =
            serde_json::from_str::<SessionizeJson>(BELFAST_JSON).unwrap();
        let event = crate::Event::try_from(sessions).unwrap();
        insta::assert_yaml_snapshot!(event);

        let mut file = Vec::<u8>::new();
        let mut cur = Cursor::new(&mut file);

        dbg!(&event);

        let t1 = event.tracks.get("Track 1 - Exhibition Centre").unwrap();
        t1.make_track(&mut cur).unwrap();

        let out = String::from_utf8(file).unwrap();
        assert_eq!(
            out,
            "\
09:45-10:25\tMorning Keynote

10:25-11:10\tWhat the Real AI Attacks of the Last
10:25-11:10\tTwo Years Actually Taught Us
10:25-11:10\t - Liana Anca Tomescu

11:30-12:15\t45,724 Networks, One Flipper Zero:
11:30-12:15\tMapping the Wireless Threat Landscape
11:30-12:15\tof Northern Ireland
11:30-12:15\t - M0r3al

12:15-13:00\tHacking Browsers: The Easy Way
12:15-13:00\t - Robbe Van Roey / PinkDraconian

13:45-14:30\tReverse Engineering an OAuth Supply
13:45-14:30\tChain Attack
13:45-14:30\t - art

14:30-15:15\tNo CVV, No 3DS, No Problem: Chaining
14:30-15:15\tTrust Failures Across a Live Aviation
14:30-15:15\tPayment Stack
14:30-15:15\t - Esat Berk Kandemir

15:40-16:25\tChaining the Unchainable: Finding and
15:40-16:25\tExploiting Logic Flaws in Modern Web
15:40-16:25\tArchitectures
15:40-16:25\t - Raj Dhruv

16:25-17:10\tLeaks on the Livewire: How a Few Blocked
16:25-17:10\tRequests Led to a Shocking Discovery
16:25-17:10\t - Daniel Johnston

17:10-17:50\tEvening Keynote
17:10-17:50\t - Aleksandra Aytova

"
        );
    }
}
