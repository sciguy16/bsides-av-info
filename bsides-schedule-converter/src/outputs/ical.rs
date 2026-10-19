use icalendar::{Calendar, Component, Event, EventLike};

impl From<crate::Event> for icalendar::Calendar {
    fn from(value: crate::Event) -> Self {
        let mut ret = Calendar::new();
        for (track_name, shows) in value.tracks {
            for session in shows.sessions {
                // Yep, we have to use this because no borrowing iterators
                // let this_event = ;
                ret.push(icalendar::Event::new()
                    .summary(&session.title)
                    .description(&session.description)
                    .starts(session.start)
                    .ends(session.end.into())
                );
            }
        }
        ret
    }
}
