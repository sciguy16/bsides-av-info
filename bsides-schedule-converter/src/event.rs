//! You should fill an Event from your input (which won't care about order much), run Event::index,
//! and then use the EventIndex for your output (which will be nicely time ordered and indexable).
//!
//! The Event type implements Serialize and Deserialize to keep the tests working

use std::{collections::BinaryHeap, fmt::Debug, ops::{Deref, Index}};

use chrono::{DateTime, NaiveDate, NaiveTime, TimeDelta, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::tz::TzOrOffset;

/// Wrapper to ensure that we got the track index from a sane source
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct TrackId(pub usize);

/// Wrapper to ensure that we got the room index from a sane source
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct RoomId(pub usize);

/// Wrapper to ensure that we got the session index from a sane source
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct SessionId(usize);

// Both of these are currently single elements. This does mean that we get a (currently) pointless
// double allocation, but means that adding extra stuff, such as accessibility/AV per-room info,
// or track descriptions, or whatever, can be done really easily by just adding a field.

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
pub struct TrackInfo {
    pub name: String
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[derive(Serialize, Deserialize)]
pub struct RoomInfo {
    pub name: String
}

/// Iterates over the bits in a trackset
pub struct TrackSetIterator {
    remaining_tracks: u64,
    pos: usize
}
impl Iterator for TrackSetIterator {
    type Item = TrackId;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.remaining_tracks == 0 {
                return None
            }
            if self.remaining_tracks & 1 != 0 {
                let ret = Some(TrackId(self.pos));
                self.remaining_tracks >>= 1;
                self.pos += 1;
                return ret
            }
            else {
                self.remaining_tracks >>= 1;
                self.pos += 1;
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}
impl ExactSizeIterator for TrackSetIterator {
    fn len(&self) -> usize {
        self.remaining_tracks.count_ones() as usize
    }
}

/// Reference the tracks without having to do a dynamic allocation each time (yay!
///
/// XXX: supports a *maximum* of 64 tracks. Why do you have more than this? Please genuinely tell me.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Serialize, Deserialize)]
pub struct TrackSet(u64);
impl TrackSet {
    /// Create an empty trackset
    pub const fn new() -> Self { Self(0) }
    pub const fn add(&mut self, track: TrackId) {
        self.0 |= 1 << track.0
    }
    pub const fn remove(&mut self, track: TrackId) {
        self.0 &= !(1 << track.0)
    }
    pub const fn contains(&self, track: TrackId) -> bool {
        self.0 & (1<<track.0) != 0
    }
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }
    pub fn iter(&self) -> TrackSetIterator {
        TrackSetIterator { remaining_tracks: self.0, pos: 0 }
    }
    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }
}
impl IntoIterator for TrackSet {
    type Item = TrackId;

    type IntoIter = TrackSetIterator;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl FromIterator<TrackId> for TrackSet {
    fn from_iter<T: IntoIterator<Item = TrackId>>(iter: T) -> Self {
        let mut ret = Self::new();
        for i in iter {
            ret.add(i);
        }
        ret
    }
}

/// All of the information on a session that could be relevant to any output
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub title: String,
    pub description: String,
    pub speakers: Vec<String>,
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
    /// The tracks that this session is in
    pub tracks: TrackSet,
    /// The physical location of this session. If this is none, just use the tracks with commas between them
    pub room: Option<RoomId>,
}
impl Session {
    pub fn clashes_with(&self, other: &Self) -> bool {
        self.end_utc >= other.start_utc && self.start_utc <= other.end_utc
    }
    pub fn runs_during(&self, time: chrono::DateTime<Utc>) -> std::cmp::Ordering {
        if time < self.start_utc {
            std::cmp::Ordering::Less
        }
        else if time > self.end_utc {
            std::cmp::Ordering::Greater
        }
        else {
            std::cmp::Ordering::Equal
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct TimeOrderedSession(Session);
impl PartialEq for TimeOrderedSession {
    fn eq(&self, other: &Self) -> bool {
        self.0.start_utc == other.0.start_utc
    }
}
impl Eq for TimeOrderedSession {}
impl Ord for TimeOrderedSession {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Revese the order because rust uses max heaps for some reason
        //
        // This will be stable AS LONG AS you don't put two things on the same tracks with the same start time with the same title
        //
        // obviously
        self.0.start_utc.cmp(&other.0.start_utc)
        .then_with(|| self.0.tracks.0.cmp(&other.0.tracks.0))
        // We could remove this, for some nebulous perf gain and a lack of stability when someone screws up tracks
        .then_with(|| self.0.title.cmp(&other.0.title))
    }
}
impl PartialOrd for TimeOrderedSession {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Put all of your events in here
//
// This has a hard invariant: we must make sure that no index changes, so ONLY APPEND!!!
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct Event {
    pub time_zone: TzOrOffset,
    // Name indexed tracks
    tracks: IndexMap<String, TrackInfo>,
    // Name indexed rooms
    rooms: IndexMap<String, RoomInfo>,
    sessions: BinaryHeap<TimeOrderedSession>,
}
impl Event {
    pub fn new(time_zone: TzOrOffset) -> Self {
        Self {
            time_zone,
            tracks: Default::default(),
            rooms: Default::default(),
            sessions: Default::default(),
        }
    }

    pub fn get_room_by_name(&self, name: &str) -> Option<RoomId> {
        self.rooms.get_index_of(name).map(RoomId)
    }
    pub fn get_track_by_name(&self, name: &str) -> Option<TrackId> {
        self.tracks.get_index_of(name).map(TrackId)
    }
    pub fn get_or_create_room<Name: AsRef<str>>(&mut self, name: Name, create: impl FnOnce(Name) -> RoomInfo) -> RoomId {
        self.get_room_by_name(name.as_ref()).unwrap_or_else(|| {
            let ret = RoomId(self.rooms.len());
            let room = create(name);
            if self.rooms.insert(room.name.clone(), room).is_some() {
                panic!("Provided room name collided with created room name");
            }
            ret
        })
    }
    pub fn get_or_create_track<Name: AsRef<str>>(&mut self, name: Name, create: impl FnOnce(Name) -> TrackInfo) -> TrackId {
        self.get_track_by_name(name.as_ref()).unwrap_or_else(|| {
            let ret = TrackId(self.tracks.len());
            let track = create(name);
            if self.tracks.insert(track.name.clone(), track).is_some() {
                panic!("Provided track name collided with created track name");
            }
            ret
        })
    }
    // We don't return a "SessionIdx", because sessions do not need to refer to each other
    // and we don't want to have to juggle a nice "SessionIdx" with an evil "OptionSessionIdx"
    pub fn add_session(&mut self, session: Session) {
        self.sessions.push(TimeOrderedSession(session));
    }

    // We take ownership to make sure that noone invalidates our index
    pub fn finish(self) -> EventIndex {
        EventIndex::new(self)
    }
}
impl Index<TrackId> for Event {
    type Output = TrackInfo;

    fn index(&self, index: TrackId) -> &Self::Output {
        // This index must be valid unless it came from another event
        &self.tracks[index.0]
    }
}
impl Index<RoomId> for Event {
    type Output = RoomInfo;

    fn index(&self, index: RoomId) -> &Self::Output {
        // This index must be valid unless it came from another event
        &self.rooms[index.0]
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoomIndex {
    pub info: RoomInfo,
    pub sessions: Vec<SessionId>
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrackIndex {
    pub info: TrackInfo,
    pub sessions: Vec<SessionId>
}

#[derive(Clone, Copy)]
pub struct IndexedSession<'a> {
    event: &'a EventIndex,
    session: &'a Session
}
impl<'a> IndexedSession<'a> {
    #[inline(always)]
    pub fn event(&self) -> &'a EventIndex {
        self.event
    }
    /// Get the start time in the event-local time zone
    pub fn start(&self) -> DateTime<TzOrOffset> {
        self.session.start_utc.with_timezone(&self.event.time_zone)
    }
    /// Get the end time in the event-local time zone
    pub fn end(&self) -> DateTime<TzOrOffset> {
        self.session.end_utc.with_timezone(&self.event.time_zone)
    }
    /// Get the index of the day in the event
    pub fn day(&self) -> u64 {
        let ret = (self.start() - self.event.start()).num_days();
        // This cannot be negative, as we came from a sorted list :(
        debug_assert!(ret >= 0, "Got an indexed session before the start of the event");
        ret as u64
    }
    /// Get a sequence of `day{day + 1}-{track_name}` that this session is in, skipping empty components
    pub fn maybe_day_track_name(&self) -> impl ExactSizeIterator<Item=String> {
        if self.event().single_day() {
            if self.event().tracks().len() == 1 {
                either::Left(either::Left(std::iter::once(Default::default())))
            }
            else {
                either::Left(either::Right(self.tracks.iter().map(move |track_id| self.event()[track_id].name.clone())))
            }
        }
        else {
            if self.event().tracks().len() == 1 {
                either::Right(either::Left(std::iter::once(format!("day{}", self.day() + 1))))
            }
            else {
                either::Right(either::Right(self.tracks.iter().map(|track_id| format!("day{}-{}", self.day() + 1, self.event()[track_id].name))))
            }
        }
    }
}
impl Debug for IndexedSession<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("IndexedSession").field(&self.session).finish()
    }
}
impl<'a> Deref for IndexedSession<'a> {
    type Target = &'a Session;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}
impl PartialEq for IndexedSession<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.session == other.session
    }
}
impl Eq for IndexedSession<'_> {}

/// Sorted events, that you can use something like index[track_id].sessions.map(EventIndex.index).binary_search_with(|i| i.runs_during(time))
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventIndex {
    pub time_zone: TzOrOffset,
    // Sessions sorted by time
    sessions: Vec<Session>,
    tracks: IndexMap<String, TrackIndex>,
    rooms: IndexMap<String, RoomIndex>,

    // These'll be referenced a lot, so we might as well precompute them
    #[serde_as(as = "crate::tz::Rfc9557DateTime")]
    start: DateTime<TzOrOffset>,
    #[serde_as(as = "crate::tz::Rfc9557DateTime")]
    end: DateTime<TzOrOffset>,
    last_day: NaiveDate
}
impl EventIndex {
    /// Get the session with the given id
    //
    // Since they should have got the SessionId from us, any out of bounds access is a serious logic error,
    // so we don't give them a chance to recover
    pub fn session<'a>(&'a self, id: SessionId) -> IndexedSession<'a> {
        IndexedSession { event: self, session: &self.sessions[id.0] }
    }
    /// Returns the tracks in SessionId and time sorted order
    pub fn sessions<'a>(&'a self) -> impl ExactSizeIterator<Item=(SessionId, IndexedSession<'a>)> + DoubleEndedIterator {
        self.sessions.iter().enumerate().map(|(idx, i)| (SessionId(idx), IndexedSession{event: self, session: i}))
    }
    /// Returns the tracks in TrackId sorted order
    pub fn tracks(&self) -> impl ExactSizeIterator<Item=(TrackId, &TrackIndex)> + DoubleEndedIterator {
        self.tracks.iter().enumerate().map(|(idx, (_, i))| (TrackId(idx), i))
    }
    /// Returns the tracks in RoomId sorted order
    pub fn rooms(&self) -> impl ExactSizeIterator<Item=(RoomId, &RoomIndex)> + DoubleEndedIterator {
        self.rooms.iter().enumerate().map(|(idx, (_, i))| (RoomId(idx), i))
    }
    /// The start time of the first event, in the event-local time zone
    pub fn start(&self) -> DateTime<TzOrOffset> { self.start }
    /// The end time of the last event, in the event-local time zone
    pub fn end(&self) -> DateTime<TzOrOffset> { self.end }
    /// The last day of the event (checks for midnight)
    pub fn last_day(&self) -> NaiveDate { self.last_day }
    /// A more efficiently way than `self.days.len()` to check if there is only a single day in the event
    pub fn single_day(&self) -> bool { self.start.date_naive() == self.last_day }
    /// Iterate over each day the event runs
    pub fn days(&self) -> impl ExactSizeIterator<Item=NaiveDate> + DoubleEndedIterator {
        let first = self.start().date_naive();
        let mut last_dt = self.end();
        // Handle event ending on midnight: we don't get an extra day smh
        if last_dt.time() == NaiveTime::MIN {
            last_dt -= TimeDelta::seconds(1);
        }
        let last = last_dt.date_naive();
        let n_days_minus_1 = (last - first).num_days();
        // If your event is running for more than 10 million years, then you can make a PR
        assert!(n_days_minus_1 <= (u32::MAX as i64 - 1), "Event duration {first} to {last} is more than (2^32 - 2) days (>10 million years!)");
        // Has to be a u32 exclusive range so that we get a free exact size iterator impl
        (0u32..n_days_minus_1 as u32 + 1)
        .map(move |i| first + TimeDelta::days(i as i64))
    }

    pub fn new(event: Event) -> Self {
        // Check that we actually have a session in our event
        if event.sessions.is_empty() {
            panic!("Asked to index an empty event");
        }

        // Take all of the events and sort them by time
        //
        // This (should!) optimise to a in_place_collect, so no-reallocations
        //
        // Once we get https://github.com/rust-lang/rust/issues/59278, we won't even need the intermediate vec
        let sessions: Vec<Session> = event.sessions.into_sorted_vec().into_iter().map(|i| i.0).collect();
        // We now need this index to _NEVER CHANGE_ or our cool indexes all become useless
        //
        // We could use Arc to avoid this, but it seems a shame to do that when we already have everything in a vec

        let mut tracks: IndexMap<String, TrackIndex> = event.tracks.into_iter().map(|i| (i.0, TrackIndex{info: i.1, sessions: vec![]})).collect();
        let mut rooms: IndexMap<String, RoomIndex> = event.rooms.into_iter().map(|i| (i.0, RoomIndex{info: i.1, sessions: vec![]})).collect();

        // Iterate through the events in time order, adding to the various event indexes in order
        for (idx, session) in sessions.iter().enumerate() {
            let session_id = SessionId(idx);
            for track in session.tracks.iter() {
                tracks[track.0].sessions.push(session_id);
            }
            if let Some(room) = session.room {
                rooms[room.0].sessions.push(session_id);
            }
        }

        let start =  sessions.first().unwrap().start_utc.with_timezone(&event.time_zone);
        let end = sessions.last().unwrap().end_utc.with_timezone(&event.time_zone);
        // Move back one second.
        //
        // If the entire event is zero seconds long and starts at midnight, then this will break.
        // Send a PR if you can actually justify why you would care
        let last_day = (end - TimeDelta::seconds(1)).date_naive();
        Self {
            start,
            end,
            last_day,
            time_zone: event.time_zone,
            sessions,
            tracks,
            rooms,
        }
    }
}
impl From<Event> for EventIndex {
    fn from(event: Event) -> Self {
        Self::new(event)
    }
}
// Now that it's all sorted, we can do efficient comparison
//
// If an event has the same rooms, tracks and events, then they're equal
impl PartialEq for EventIndex {
    fn eq(&self, other: &Self) -> bool {
        // Easy check first
        self.time_zone == other.time_zone &&
        // Check all the room info is the same.
        //
        // We're going to assume that both these and the tracks are in the same order, unless someone wants to make a separate id system
        self.rooms.iter().zip(&other.rooms).all(|(a, b)| a.1.info == b.1.info) &&
        // ASSUMING that there are no clashes, the sessions in a track must be in the same order, so we can actually do a direct comparison
        //
        // Asymptotically, building an intermediate translation table for events may be faster, but that's dumb so dumb comparison it is
        self.tracks.iter().zip(&other.tracks).all(|(a, b)| {
            a.1.info == b.1.info &&
            a.1.sessions.iter().zip(&b.1.sessions)
                .all(|(our_session, their_session)| self.session(*our_session) == self.session(*their_session))
        })
    }
}
impl Eq for EventIndex {}

impl Index<TrackId> for EventIndex {
    type Output = TrackInfo;

    fn index(&self, index: TrackId) -> &Self::Output {
        // This index must be valid unless it came from another event
        &self.tracks[index.0].info
    }
}
impl Index<RoomId> for EventIndex {
    type Output = RoomInfo;

    fn index(&self, index: RoomId) -> &Self::Output {
        // This index must be valid unless it came from another event
        &self.rooms[index.0].info
    }
}

// FIXME: add tests
#[cfg(test)]
pub mod test {
    use crate::inputs::{EventParser, pretalx::Pretalx};

    use super::*;
    // This is the test case that we're going to run non-parsers against. We should probably add more later...
    //
    // It has multiple tracks and multiple days, so should be enough to mess something up if you weren't careful
    //
    // Also, this means that all formatter tests will fail if the pretalx formatter dies, which isn't great
    pub fn load_bristol() -> Event {
        Pretalx.parse_from_str(include_str!("../test-data/pretalx/bristol-2026.json"))
            .expect("THIS IS A DIFFERENT ISSUE! Sessionize parser is buggy")
    }

    #[test]
    fn test_index() {
        insta::assert_yaml_snapshot!(load_bristol().finish())
    }

    #[test]
    fn test_track_loop() {
        let expected = [2, 3, 7];
        let ts: TrackSet = expected.into_iter().map(TrackId).collect();
        assert_eq!(ts.0, expected.into_iter().map(|i| 1 << i).fold(0, |acc, i| acc | i), "Bit set implementation wonky");
        let res: Vec<_> = ts.into_iter().map(|i| i.0).collect();
        assert_eq!(expected.as_slice(), res, "IntoIter impl wonky");
    }

    #[test]
    fn test_single_track_loop() {
        let expected = [0];
        let ts: TrackSet = expected.into_iter().map(TrackId).collect();
        assert_eq!(ts.0, expected.into_iter().map(|i| 1 << i).fold(0, |acc, i| acc | i), "Bit set implementation wonky");
        let res: Vec<_> = ts.into_iter().map(|i| i.0).collect();
        assert_eq!(expected.as_slice(), res, "IntoIter impl wonky");
    }
}
