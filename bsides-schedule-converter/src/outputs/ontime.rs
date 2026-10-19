use chrono::{DateTime, NaiveTime, TimeDelta};
use color_eyre::Result;
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use std::io::Write;

/*
Start 	Excel time | string 	00:00:00
Link start 	boolean 	false
End 	Excel time | string 	00:00:00
Duration 	Excel time | string 	00:00:00
Cue 	string 	“”
Title 	string 	“”
Skip 	boolean 	false
Note 	string 	“”
Colour 	string (# hex colour or named css colour) 	“”
End action 	none load-next play-next 	none
Timer type* 	count-down count-up clock block skip group group-end milestone none 	count-down
Count to end* 	boolean 	false
Time warning 	Excel time | string 	00:02:00
Time danger 	Excel time | string 	00:01:00
*/

const BREAK_COLOUR: &str = "yellow";
const TALK_COLOUR: &str = "green";

const TIME_WARN: TimeDelta = TimeDelta::minutes(5);
const TIME_DANGER: TimeDelta = TimeDelta::minutes(1);
// Time just before midnight for generating end-of-day cues
const MIDNIGHT: NaiveTime = NaiveTime::from_hms_opt(23, 59, 59).unwrap();

pub fn make_ontime_export<W: Write + Send>(
    csv_writer: W,
    xlsx_writer: W,
    track: &crate::show_file::ShowFile,
) -> Result<()> {
    let mut csv_writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(csv_writer);

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.deserialize_headers::<OntimeExport>(1, 1)?;

    let mut cue_number = 0;

    let mut intermission = crate::show_file::Session {
        start: DateTime::default(),
        end: DateTime::default(),
        is_break: true,
        title: "Intermission".into(),
        description: String::new(),
        speakers: Vec::new(),
    };

    let sessions = track
        .sessions
        .iter()
        .map(Some)
        .chain(std::iter::once(None))
        .collect::<Vec<_>>();
    for [talk1, talk2] in sessions.array_windows() {
        // Talk

        #[expect(clippy::unwrap_used)]
        let talk1 = talk1.unwrap();

        let data = OntimeExport::new(talk1, &mut cue_number);
        csv_writer.serialize(&data)?;
        worksheet.serialize(&data)?;

        // If next session does not start immediately then insert an
        // intermission
        intermission.start = talk1.end;
        if let Some(talk2) = talk2 {
            if talk1.end == talk2.start {
                continue;
            }
            intermission.end = talk2.start;
        } else {
            // make an end of day
            intermission.end = MIDNIGHT.into();
            intermission.title = "End of day".into();
        };

        let data = OntimeExport::new(&intermission, &mut cue_number);
        csv_writer.serialize(&data)?;
        worksheet.serialize(&data)?;
    }

    csv_writer.flush()?;
    workbook.save_to_writer(xlsx_writer)?;
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct OntimeExport {
    #[serde(rename = "Time Start")]
    start: String,
    #[serde(rename = "Link start")]
    link_start: bool,
    #[serde(rename = "Time End")]
    end: String,
    duration: String,
    cue: String,
    title: String,
    skip: bool,
    colour: String,
    #[serde(rename = "End action")]
    end_action: EndAction,
    #[serde(rename = "Timer type")]
    time_ty: TimerType,
    #[serde(rename = "Count to end")]
    count_to_end: bool,
    #[serde(rename = "Warning time")]
    time_warning: String,
    #[serde(rename = "Danger time")]
    time_danger: String,
}

#[derive(Copy, Clone)]
enum SessionType {
    Talk,
    Break,
}

impl SessionType {
    // link_start is TRUE for TALK sessions
    const fn link_start(&self) -> bool {
        matches!(self, Self::Talk)
    }

    fn colour(&self) -> String {
        match self {
            Self::Talk => TALK_COLOUR.into(),
            Self::Break => BREAK_COLOUR.into(),
        }
    }

    // count_to_end is FALSE for TALK sessions
    const fn count_to_end(&self) -> bool {
        matches!(self, Self::Break)
    }

    // time_ty is CountDown for talks and Clock for breaks
    fn time_ty(&self) -> TimerType {
        match self {
            Self::Talk => TimerType::CountDown,
            Self::Break => TimerType::Clock,
        }
    }
}

impl OntimeExport {
    fn new(session: &crate::show_file::Session, cue_number: &mut i32) -> Self {
        let duration = format_duration(session.end - session.start);
        *cue_number += 1;

        let ty = if session.is_break {
            SessionType::Break
        } else {
            SessionType::Talk
        };

        Self {
            start: session.start.to_string(),
            link_start: ty.link_start(),
            end: session.end.to_string(),
            duration,
            cue: cue_number.to_string(),
            title: session.title.clone(),
            skip: false,
            colour: ty.colour(),
            end_action: EndAction::None,
            time_ty: ty.time_ty(),
            count_to_end: ty.count_to_end(),
            time_warning: format_duration(TIME_WARN),
            time_danger: format_duration(TIME_DANGER)
        }
    }
}

fn format_duration(duration: TimeDelta) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        duration.num_hours(),
        duration.num_minutes() % 60,
        duration.num_seconds() % 60,
    )
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum EndAction {
    #[default]
    None,
    LoadNext,
    PlayNext,
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum TimerType {
    #[default]
    CountDown,
    CountUp,
    Clock,
    Block,
    Skip,
    Group,
    GroupEnd,
    Milestone,
    None,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::formats::{pretalx::Pretalx, sessionize_json::SessionizeJson};
    use std::io::Cursor;

    const BRISTOL: &str = include_str!("../test-data/bristol-2026.json");
    const BELFAST_JSON: &str = include_str!("../test-data/belfast.json");

    #[test]
    fn test_duration_fmt() {
        let mut duration = TimeDelta::seconds(0);
        assert_eq!(format_duration(duration), "00:00:00");
        duration += TimeDelta::seconds(15);
        assert_eq!(format_duration(duration), "00:00:15");
        duration += TimeDelta::minutes(2);
        assert_eq!(format_duration(duration), "00:02:15");
        duration += TimeDelta::minutes(2);
        assert_eq!(format_duration(duration), "00:04:15");
        duration += TimeDelta::minutes(59);
        assert_eq!(format_duration(duration), "01:03:15");
    }

    #[test]
    fn test_deserialise() {
        let _ = serde_json::from_str::<Pretalx>(BRISTOL).unwrap();
    }

    #[test]
    fn test_show_file() {
        let mut bristol = serde_json::from_str::<Pretalx>(BRISTOL).unwrap();

        let mut csv_file = Vec::<u8>::new();
        let mut csv_cur = Cursor::new(&mut csv_file);
        let mut xlsx_file = Vec::<u8>::new();
        let mut xlsx_cur = Cursor::new(&mut xlsx_file);

        let t1 = bristol.schedule.conference.days[0]
            .rooms
            .swap_remove("Track 1")
            .unwrap();
        let show = crate::show_file::ShowFile::try_from(t1).unwrap();

        make_ontime_export(&mut csv_cur, &mut xlsx_cur, &show).unwrap();

        let out = String::from_utf8(csv_file).unwrap();
        assert_eq!(
            out,
            r#"Time Start,Link start,Time End,Duration,Cue,Title,Skip,Colour,End action,Timer type,Count to end,Warning time,Danger time
08:55:00,true,09:05:00,00:10:00,1,Opening Notes Friday,false,green,none,count-down,false,00:05:00,00:01:00
09:05:00,true,09:45:00,00:40:00,2,Keynote - Managing The Minefield of Management Cyber Explosions - Understanding what to say and when to say it,false,green,none,count-down,false,00:05:00,00:01:00
09:45:00,false,10:00:00,00:15:00,3,Intermission,false,yellow,none,clock,true,00:05:00,00:01:00
10:00:00,true,10:40:00,00:40:00,4,"Fake It Till You Detect It: Live Deepfakes, Detection Gaps, and the Human Risk Management Response",false,green,none,count-down,false,00:05:00,00:01:00
10:40:00,false,11:00:00,00:20:00,5,Intermission,false,yellow,none,clock,true,00:05:00,00:01:00
11:00:00,true,11:40:00,00:40:00,6,The Next Internal Network: Why Your Old Playbook Doesn't Work,false,green,none,count-down,false,00:05:00,00:01:00
11:40:00,false,14:00:00,02:20:00,7,Intermission,false,yellow,none,clock,true,00:05:00,00:01:00
14:00:00,true,14:40:00,00:40:00,8,Domain Protection - How We Protect Our Customers,false,green,none,count-down,false,00:05:00,00:01:00
14:40:00,false,15:00:00,00:20:00,9,Intermission,false,yellow,none,clock,true,00:05:00,00:01:00
15:00:00,true,15:40:00,00:40:00,10,The biggest digital sovereignty risk isn’t what you think...,false,green,none,count-down,false,00:05:00,00:01:00
15:40:00,false,16:00:00,00:20:00,11,Intermission,false,yellow,none,clock,true,00:05:00,00:01:00
16:00:00,true,16:40:00,00:40:00,12,Getting started in embedded device research,false,green,none,count-down,false,00:05:00,00:01:00
16:40:00,false,17:00:00,00:20:00,13,Intermission,false,yellow,none,clock,true,00:05:00,00:01:00
17:00:00,true,17:20:00,00:20:00,14,How Efficiency Can Lead to Disruption: The Evolution of Phishing Emails,false,green,none,count-down,false,00:05:00,00:01:00
17:20:00,false,23:59:59,06:39:59,15,End of day,false,yellow,none,clock,true,00:05:00,00:01:00
"#,
        );
    }

    #[test]
    fn test_show_file_with_breaks() {
        let belfast =
            serde_json::from_str::<SessionizeJson>(BELFAST_JSON).unwrap();

        let mut csv_file = Vec::<u8>::new();
        let mut csv_cur = Cursor::new(&mut csv_file);
        let mut xlsx_file = Vec::<u8>::new();
        let mut xlsx_cur = Cursor::new(&mut xlsx_file);

        let event = crate::Event::try_from(belfast).unwrap();
        make_ontime_export(
            &mut csv_cur,
            &mut xlsx_cur,
            event.tracks.get("Track 1 - Exhibition Centre").unwrap(),
        )
        .unwrap();

        let out = String::from_utf8(csv_file).unwrap();
        assert_eq!(
            out,
            r#"Time Start,Link start,Time End,Duration,Cue,Title,Skip,Colour,End action,Timer type,Count to end,Warning time,Danger time
08:30:00,false,09:30:00,01:00:00,1,Registration (Grand Ballroom Entrance),false,yellow,none,clock,true,00:05:00,00:01:00
09:30:00,false,09:45:00,00:15:00,2,Opening remarks,false,yellow,none,clock,true,00:05:00,00:01:00
09:45:00,true,10:25:00,00:40:00,3,Morning Keynote,false,green,none,count-down,false,00:05:00,00:01:00
10:25:00,true,11:10:00,00:45:00,4,What the Real AI Attacks of the Last Two Years Actually Taught Us,false,green,none,count-down,false,00:05:00,00:01:00
11:10:00,false,11:30:00,00:20:00,5,Morning break,false,yellow,none,clock,true,00:05:00,00:01:00
11:30:00,true,12:15:00,00:45:00,6,"45,724 Networks, One Flipper Zero: Mapping the Wireless Threat Landscape of Northern Ireland",false,green,none,count-down,false,00:05:00,00:01:00
12:15:00,true,13:00:00,00:45:00,7,Hacking Browsers: The Easy Way,false,green,none,count-down,false,00:05:00,00:01:00
13:00:00,false,13:45:00,00:45:00,8,Lunch,false,yellow,none,clock,true,00:05:00,00:01:00
13:45:00,true,14:30:00,00:45:00,9,Reverse Engineering an OAuth Supply Chain Attack,false,green,none,count-down,false,00:05:00,00:01:00
14:30:00,true,15:15:00,00:45:00,10,"No CVV, No 3DS, No Problem: Chaining Trust Failures Across a Live Aviation Payment Stack",false,green,none,count-down,false,00:05:00,00:01:00
15:15:00,false,15:40:00,00:25:00,11,Afternoon break,false,yellow,none,clock,true,00:05:00,00:01:00
15:40:00,true,16:25:00,00:45:00,12,Chaining the Unchainable: Finding and Exploiting Logic Flaws in Modern Web Architectures,false,green,none,count-down,false,00:05:00,00:01:00
16:25:00,true,17:10:00,00:45:00,13,Leaks on the Livewire: How a Few Blocked Requests Led to a Shocking Discovery,false,green,none,count-down,false,00:05:00,00:01:00
17:10:00,true,17:50:00,00:40:00,14,Evening Keynote,false,green,none,count-down,false,00:05:00,00:01:00
17:50:00,false,18:10:00,00:20:00,15,Closing Remarks,false,yellow,none,clock,true,00:05:00,00:01:00
18:10:00,false,18:30:00,00:20:00,16,Intermission,false,yellow,none,clock,true,00:05:00,00:01:00
18:30:00,false,23:00:00,04:30:00,17,"After Party (Haymarket, wristbands required)",false,yellow,none,clock,true,00:05:00,00:01:00
23:00:00,false,23:59:59,00:59:59,18,End of day,false,yellow,none,clock,true,00:05:00,00:01:00
"#,
        );
    }
}
