#![cfg_attr(not(test), warn(clippy::unwrap_used))]

use clap::Parser;
use color_eyre::Result;

use crate::{event::{Event, EventIndex}, inputs::DEFAULT_PARSERS, outputs::default_formatters};

mod tz;
mod event;
mod inputs;
mod outputs;
// mod wordlist;

#[cfg(test)]
#[macro_use(assert_eq)]
extern crate pretty_assertions;

#[derive(Parser)]
pub struct Args {
    location: String,
    output_dir: std::path::PathBuf
}

// Separated out for easier testing
pub fn main_inner(args: Args) -> Result<()> {
    println!("game");
    std::fs::create_dir_all(&args.output_dir)?;
    color_eyre::install()?;
    let event = Event::parse(&args.location, DEFAULT_PARSERS)?;
    println!("Indexing");
    let event: EventIndex = event.into();
    for formatter in default_formatters() {
        // If a formatter fails, we can keep going
        if let Err(e) = formatter.format(&event, &args.output_dir) {
            println!("{formatter:?} failed: {e}");
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    main_inner(Args::parse())



    // let sched = std::fs::read_to_string(&args.location)?;
    // println!("Loaded file `{}`", args.location.display());
    // let schedule = formats::Format::parse_from_str(&sched)?;
    // let event = Event::try_from(schedule)?;
    // println!("Loaded event with {} tracks", event.tracks.len());
    // event.make_show_files()?;

    // let wordlist_file = "wordlist.txt";
    // let file = std::fs::File::create(wordlist_file)?;
    // wordlist::make_wordlist_file(file, &event)?;
    // println!("Wordlist written to `{wordlist_file}`");
}
