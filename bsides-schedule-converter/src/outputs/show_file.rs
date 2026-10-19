use color_eyre::Result;
use itertools::Itertools;
use std::{fmt::Write as FmtWrite, io::Write, path::Path};

use crate::outputs::{EventFormatter, FileTracker};

/// TODO: actually do this intelligently
const TITLE_WRAP: usize = 35;

#[derive(Debug)]
pub struct ShowFile;
impl EventFormatter for ShowFile {
    fn format(&self, event: &crate::event::EventIndex, output_path: &Path) -> Result<()> {
        let mut files = FileTracker::new(output_path.to_owned());
        for (session_id, talk) in event.sessions() {
            let mut data = String::new();
            let start = talk.start();
            // TODO: no way do we end up straddling midnight?
            let end = talk.end();
            let times = format_args!(
                "{}-{}",
                start.format("%H:%M"),
                end.format("%H:%M"),
            );

            let title_lines = wrap_title(&talk.title);

            for line in title_lines {
                writeln!(&mut data, "{times}\t{line}")?;
            }
            for speaker_name in &talk.speakers {
                writeln!(&mut data, "{times}\t - {speaker_name}")?;
            }
            writeln!(&mut data)?;

            let a = talk.maybe_day_track_name().collect_vec();
            if a.is_empty() {
                panic!("{session_id:?}: {talk:?}")
            }

            for name in talk.maybe_day_track_name() {
                println!("{session_id:?}: ({}, {}) -> {name}", talk.day()+1, talk.tracks.iter().map(|i| &event[i].name).join(", "));
                write!(files.open(name.to_string())?, "{}", data)?;
            }
        }
        Ok(())
    }
}

// Wrap the title at the first space after 30 chars
fn wrap_title(title: &str) -> Vec<String> {
    let mut title_lines = Vec::new();
    let mut counter = 0;
    let mut line = String::new();
    for chr in title.chars() {
        counter += 1;
        if counter >= TITLE_WRAP && chr == ' ' {
            title_lines.push(std::mem::take(&mut line));
            counter = 0;
        } else {
            line.push(chr);
        }
    }
    if line.len() > 1 {
        title_lines.push(line);
    }
    title_lines
}


#[cfg(test)]
mod test {
    use super::ShowFile;
    use crate::{event::test::load_bristol, outputs::test::test_formatter, test_data_path};

    #[test]
    fn test_show_file() {
        test_formatter(&ShowFile, load_bristol().finish(), &test_data_path!("show-file"));
    }
}
