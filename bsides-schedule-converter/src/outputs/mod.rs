use std::{collections::BTreeMap, fmt::Debug, fs::File, path::PathBuf};

use color_eyre::Result;

use crate::event::EventIndex;

// pub mod ical;
// pub mod ontime;
pub mod show_file;
pub mod wordlist;

/// Enables a slightly less slow version of opening and appending each time
///
/// XXX: assumes that all files in in the same directory, and will panic if `/` is found in the string
/// (for protection against path traversal)
pub struct FileTracker {
    output_dir: PathBuf,
    file_cache: BTreeMap<String, File>
}
impl FileTracker {
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            file_cache: Default::default()
        }
    }
    pub fn open(&mut self, name: impl AsRef<str> + Into<String>) -> Result<&mut File> {
        assert!(!name.as_ref().contains('/'), "File traversal attack detected in generated file name {:?}", name.as_ref());
        // TODO: optimise this forced allocation away if needed
        match self.file_cache.entry(name.into()) {
            std::collections::btree_map::Entry::Occupied(occupied_entry) => Ok(occupied_entry.into_mut()),
            std::collections::btree_map::Entry::Vacant(vacant_entry) => {
                let file = std::fs::File::create(self.output_dir.join(vacant_entry.key()))?;
                Ok(vacant_entry.insert(file))
            },
        }
    }
}

pub trait EventFormatter: Debug {
    fn format(&self, event: &EventIndex, output_dir: &std::path::Path) -> Result<()>;
}

pub fn default_formatters() -> Vec<Box<dyn EventFormatter>> {
    vec![
        Box::new(show_file::ShowFile),
        Box::new(wordlist::WordList::with_system_dicts())
    ]
}

#[cfg(test)]
mod test {
    use color_eyre::Result;
    use itertools::Itertools;
    use std::{collections::BinaryHeap, ffi::OsStr, path::{Path, PathBuf}};

    use super::*;

    /// Get files in a directory and return them in lexicographical order
    ///
    /// Because of https://github.com/rust-lang/rust/issues/59278, we can't use the more sane choice of a binary heap
    pub fn get_files(dir: &Path) -> Result<BinaryHeap<PathBuf>> {
        std::fs::read_dir(dir)?
        .map(|dirent| {
            let dirent = dirent?;
            if !dirent.file_type()?.is_file() {
                Ok(None)
            }
            else {
                Ok(Some(dirent.path()))
            }
        })
        .process_results(|i| i.flatten().collect())
    }
    /// Check the contents are the same
    //
    // We could do this a lot more efficiently (two passes with ordered iterators),
    // but this is easier to check that I haven't made a mistake :p
    fn assert_same_files(output_dir: &Path, compare_dir: &Path) {
        // Get a big list of each collection of files, because that makes formatting wayyyy easier
        let generated_files: BTreeMap<PathBuf, PathBuf> =
            get_files(output_dir).expect("Failed to enumerate generated files").into_sorted_vec().into_iter()
            .map(|i|  (i.strip_prefix(output_dir).unwrap().to_owned(), i))
            .collect();
        let expected_files: BTreeMap<PathBuf, PathBuf> =
            get_files(compare_dir).expect("Failed to enumerate expected files").into_sorted_vec().into_iter()
            .map(|i|  (i.strip_prefix(compare_dir).unwrap().to_owned(), i))
            .collect();

        println!("Generated:\n{}", generated_files.keys().format_with("\n", |i, f| f(&format_args!("    {:#?}", i))));
        println!("Expected:\n{}", expected_files.keys().format_with("\n", |i, f| f(&format_args!("    {:#?}", i))));

        // Yes, we could be smarter than this, but that's a lot of effort to save a ~O(n) pass over a small number of small strings
        let files: Vec<&PathBuf> = generated_files.keys().merge(expected_files.keys()).dedup().collect();

        for file in files {
            let generated_path = generated_files.get(file);
            let expected_path = expected_files.get(file);
            // Check the file contents are the same (both rel paths are the same here)
            let generated_file = generated_path.map(std::fs::read).transpose().expect("Could not read generated file");
            let expected_file = expected_path.map(std::fs::read).transpose().expect("Could not read comparison file");
            if generated_file.is_none() {
                println!("{file:?} was not generated");
            }
            else if expected_file.is_none() {
                println!("{file:?} was not supposed to have been generated");
            }
            // For nicer formatting, see if we can make them strings
            if
                let Ok(generated_file) = generated_file.as_ref().map(|i| std::str::from_utf8(i)).unwrap_or(Ok("")) &&
                let Ok(expected_file) = expected_file.as_ref().map(|i| std::str::from_utf8(i)).unwrap_or(Ok(""))
            {
                assert_eq!(generated_file, expected_file, "The generated {file:?} was different that the expected file");
            }
            else {
                assert_eq!(generated_file, expected_file, "The generated {file:?} was different that the expected file");
            }
            // Make sure that we don't compare empty files as equal to empty strings
            if generated_file.is_none() != expected_file.is_none()  {
                // We printed an error message earlier
                panic!()
            }
            println!("Generated {file:?} matched.");
        }
    }
    /// Test a formatter against a known output
    pub fn test_formatter(format: &impl EventFormatter, input: EventIndex, compare_dir: &(impl AsRef<OsStr> + ?Sized)) {
        // I'm making the executive decision that we don't care if somehow something creates empty dirs,
        // and only compare files + paths.
        //
        // If you've screwed up so hard that this is an issue for you, consider fixing your code with GNU shred

        // Create a scoped handle for somewhere to dump our generated data
        let output_dir = tempfile::tempdir().expect("Could not create temporary directory for testing formatter");
        format.format(&input, output_dir.path()).expect("Failed to format input");
        assert_same_files(output_dir.path(), Path::new(compare_dir));
    }
    #[macro_export]
    macro_rules! test_data_path {
        ($name: literal) => {
            // {
            //     let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            //     dir.push(std::concat!("test-data/", $name));
            //     dir
            // }
                std::concat!(env!("CARGO_MANIFEST_DIR"), "/test-data/", $name)
        };
    }
    #[test]
    fn test_assert_same_files() {
        // The source directory should be equal to itself
        let mut files = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // We're in a workspace, so we have to go up once (https://github.com/rust-lang/cargo/issues/3946)
        files.pop();
        files.push(file!());
        files.pop();
        println!("Comparing source directory {files:?} with iteself");
        // If this somehow damages source files imma be so angry
        assert_same_files(&files, &files);
    }
}
