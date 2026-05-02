use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let path = Path::new(&out_dir).join("irregular_verbs.rs");
    let mut file = BufWriter::new(File::create(&path).unwrap());

    let mut map = phf_codegen::Map::new();
    let mut keys = HashSet::new();

    // Closure to track duplicates and insert into the map safely
    let mut insert = |key: &str, value: &str| {
        if keys.insert(key.to_string()) {
            map.entry(key.to_string(), &format!("\"{}\"", value));
        }
    };

    // Add defective modal verbs manually to ensure they take precedence
    // over colliding "s" forms (like 'cans' and 'wills')
    let modals = [
        "can", "could", "will", "would", "shall", "should", "may", "might", "must",
    ];
    for modal in modals {
        insert(modal, modal);
    }

    // Explicitly add forms of 'be' just in case
    insert("be", "is");
    insert("was", "was");
    insert("is", "is");

    // Helper macro to read and insert verbs from a JSON file
    macro_rules! process_file {
        ($file_path:expr) => {
            if let Ok(file_in) = File::open($file_path) {
                let reader = BufReader::new(file_in);
                if let Ok(verbs) = serde_json::from_reader::<_, Vec<Vec<Option<String>>>>(reader) {
                    for entry in verbs {
                        if entry.len() >= 2 {
                            if let (Some(base), Some(third_person)) = (&entry[0], &entry[1]) {
                                insert(base, third_person);
                            }
                        }
                        if entry.len() >= 3 {
                            if let Some(past) = &entry[2] {
                                // Inserting the past tense form natively maps [source:ran] to 'ran'
                                insert(past, past);
                            }
                        }
                    }
                }
                println!("cargo:rerun-if-changed={}", $file_path);
            }
        };
    }

    // Process both JSON files
    process_file!("data/irregular_verbs.json");
    process_file!("data/colliding_verbs.json");

    writeln!(
        &mut file,
        "#[allow(clippy::unreadable_literal)]\nstatic IRREGULAR_VERBS: phf::Map<&'static str, &'static str> = {};",
        map.build()
    )
    .unwrap();
}
