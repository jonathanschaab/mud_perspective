use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = env::var("OUT_DIR")?;
    let path = Path::new(&out_dir).join("irregular_verbs.rs");
    let mut file = BufWriter::new(File::create(&path)?);

    let mut map = phf_codegen::Map::new();
    let mut keys = HashSet::new();

    // Closure to track duplicates and insert into the map safely
    let mut insert = |key: &str, present: &str, past: &str| {
        if keys.insert(key.to_string()) {
            map.entry(key.to_string(), format!("({:?}, {:?})", present, past));
        }
    };

    // Add defective modal verbs manually to ensure they take precedence
    // over colliding "s" forms (like 'cans' and 'wills')
    let modals = [
        ("can", "can", "could"),
        ("could", "could", "could"),
        ("will", "will", "would"),
        ("would", "would", "would"),
        ("shall", "shall", "should"),
        ("should", "should", "should"),
        ("may", "may", "might"),
        ("might", "might", "might"),
        ("must", "must", "must"),
        ("ought", "ought", "ought"),
    ];
    for (modal, present, past) in modals {
        insert(modal, present, past);
    }

    // Explicitly add forms of 'be' just in case
    insert("be", "is", "was");

    let mut collisions: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    // Helper macro to read and insert verbs from a JSON file
    macro_rules! process_file {
        ($file_path:expr, $is_colliding:expr) => {
            println!("cargo:rerun-if-changed={}", $file_path);
            let file_in = File::open($file_path)?;
            let reader = BufReader::new(file_in);
            let verbs: Vec<Vec<Option<String>>> = serde_json::from_reader(reader)?;
            for entry in verbs {
                if let [Some(base), Some(third_person), rest @ ..] = entry.as_slice() {
                    let past = if let [Some(p), ..] = rest {
                        p
                    } else {
                        third_person
                    };

                    if $is_colliding {
                        let annotated = format!("{base}({past})");
                        insert(&annotated, third_person, past);

                        let list = collisions.entry(base.clone()).or_default();
                        if !list.contains(past) {
                            list.push(past.clone());
                        }
                    }

                    // Always insert the base verb (first-come, first-served)
                    insert(base, third_person, past);
                }
            }
        };
    }

    // Process both JSON files
    process_file!("data/irregular_verbs.json", false);
    process_file!("data/colliding_verbs.json", true);

    let mut collision_map = phf_codegen::Map::new();
    for (base, pasts) in collisions {
        if pasts.len() > 1 {
            let pasts_str = pasts
                .iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<_>>()
                .join(", ");
            collision_map.entry(base, format!("&[{}]", pasts_str));
        }
    }

    writeln!(
        &mut file,
        "#[allow(clippy::unreadable_literal)]\nstatic IRREGULAR_VERBS: phf::Map<&'static str, (&'static str, &'static str)> = {};",
        map.build()
    )?;

    writeln!(
        &mut file,
        "/// A compile-time map of ambiguous base verbs to their possible past tense conjugations.\n#[allow(clippy::unreadable_literal)]\npub static COLLIDING_VERBS: phf::Map<&'static str, &'static [&'static str]> = {};",
        collision_map.build()
    )?;
    Ok(())
}
