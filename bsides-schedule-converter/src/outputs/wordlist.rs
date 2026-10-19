//! Build a wordlist from the schedule for customising Azure speech-to-text.
//!
//! Take the talk titles and speaker names and directly include them in the
//! list. For the descriptions, pull out capitalised words and words not
//! present in `/usr/share/dict/words` to select the likely technical terms.

use color_eyre::{Result, eyre::eyre};
use std::{collections::{BTreeSet, HashSet}, io::{BufRead, BufReader, Write}};

use crate::outputs::{EventFormatter, FileTracker};


#[derive(Debug)]
pub struct WordList {
    dict: HashSet<String>
}
impl WordList {
    pub fn new(dict: HashSet<String>) -> Self {
        WordList {
            dict
        }
    }
    pub fn with_system_dicts() -> Self {
        Self {
            dict: load_usr_share_dict_words().map_err(|e| eyre!("Failed to load user dicts. Have you installed wbritish and wamerican?\n{e}")).unwrap()
        }
    }
    fn build_with(&self, event: &crate::event::EventIndex) -> Result<String> {
        // Talk titles and speakers
        let mut titles = BTreeSet::new();
        let mut speakers = BTreeSet::new();
        let mut words = BTreeSet::new();

        for (_, session) in event.sessions() {
            titles.insert(session.title.as_ref());
            speakers.extend(session.speakers.iter().map(String::as_str));

            for word in session.description.split_whitespace() {
                // Remove leading and trailing punctuation
                let word = word.trim_matches(|chr: char| !chr.is_alphanumeric());
                if word.len() > 2
                    && (word.chars().all(|chr| chr.is_uppercase())
                        || !self.dict.contains(&word.to_lowercase()))
                {
                    words.insert(word);
                }
            }
        }

        // We want a trailing newline, so we can't use join
        Ok(titles.into_iter().chain(speakers).chain(words).fold(String::new(), |acc, i| acc + i + "\n"))
    }
}
impl EventFormatter for WordList {
    fn format(&self, event: &crate::event::EventIndex, output_dir: &std::path::Path) -> Result<()> {
        let ret = self.build_with(event)?;

        // Slightly more convenient to use this
        let mut files = FileTracker::new(output_dir.to_owned());
        files.open("wordlist.txt")?.write_all(ret.as_bytes())?;
        Ok(())
    }
}


fn load_usr_share_dict_words() -> std::io::Result<HashSet<String>> {
    let wbritish = std::fs::File::open("/usr/share/dict/british-english")?;
    let wamerican = std::fs::File::open("/usr/share/dict/american-english")?;
    let wbritish = BufReader::new(wbritish);
    let wamerican = BufReader::new(wamerican);
    wbritish
        .lines()
        .chain(wamerican.lines())
        .map(|line| Ok(line?.to_lowercase()))
        .collect()
}

#[cfg(test)]
mod test {
    use super::WordList;
    use crate::event::test::load_bristol;

    #[test]
    fn test_bristol() {
        // FIXME: actually add a test dictionary so that the output is stable and useful
        //
        // test_formatter(&WordList::new(test_dict), load_bristol().finish(), test_data_path!("wordlist/bristol-2026/"));
        WordList::new(Default::default()).build_with(&load_bristol().finish()).unwrap();
    }
}
