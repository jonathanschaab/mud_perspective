use mud_perspective::debug::{
    DebugEntity, EntitiesPayload, generate_template_permutations,
    test_template_with_standard_entities,
};
use mud_perspective::engine::{PerspectiveEngine, Template};
use mud_perspective::models::{ActorStance, Gender, RenderContext, TemplateEntity, Tense};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;

#[derive(serde::Deserialize)]
struct TestContextDef {
    viewer_id: String,
    stance: ActorStance,
    tense: Tense,
    entities: std::collections::HashMap<String, DebugEntity>,
}

fn print_usage_and_exit(bin_name: &str) -> ! {
    eprintln!(
        "Usage: {} [input_template_file] [output_file] [--entities <json_file>] [--bind <key=subset>] [--interactive | -i] [--lookahead | -l]",
        bin_name
    );
    eprintln!("  [input_template_file] : Path to a text file containing the raw template.");
    eprintln!("  [output_file]         : (Optional) Path to save the generated permutations.");
    eprintln!("                          If omitted, results are printed to standard output.");
    eprintln!(
        "  --entities <json_file>: (Optional) Path to a JSON file containing an array of custom entities."
    );
    eprintln!(
        "  --bind <key=subset>   : (Optional) Bind a template key to a specific entity subset (e.g. --bind weapon=objects)."
    );
    eprintln!(
        "  --interactive, -i     : Start in interactive mode. This is the default if no input file is provided."
    );
    eprintln!(
        "  --lookahead, -l       : Enable lookahead (AST Pre-Pass) to test omniscient collision resolution."
    );
    process::exit(1);
}

fn evaluate_template(
    template: &Template,
    custom_payload: Option<&EntitiesPayload>,
    specific_context: Option<&TestContextDef>,
    bindings: &std::collections::HashMap<String, String>,
    lookahead: bool,
) -> Result<Vec<String>, String> {
    if let Some(ctx_def) = specific_context {
        let mut ctx = RenderContext::new(&ctx_def.viewer_id)
            .with_stance(ctx_def.stance)
            .with_tense(ctx_def.tense)
            .with_lookahead(lookahead);

        for (key, entity) in &ctx_def.entities {
            ctx = ctx.with_entity(key, entity as &dyn TemplateEntity);
        }

        let lookahead_str = if lookahead { ", Lookahead" } else { "" };
        let mut desc = format!(
            "[{:?}, {:?}{}] {{",
            ctx_def.stance, ctx_def.tense, lookahead_str
        );

        let mut first = true;
        for key in &template.template_keys {
            if let Some(entity) = ctx_def.entities.get(key) {
                if !first {
                    desc.push_str(", ");
                }
                desc.push_str(key);
                desc.push_str(": ");
                desc.push_str(&entity.display_name_for(mud_perspective::models::NULL_VIEWER));
                first = false;
            }
        }
        desc.push_str("} -> ");

        match PerspectiveEngine::render(template, &ctx) {
            Ok(res) => {
                desc.push_str(&res);
                Ok(vec![desc])
            }
            Err(e) => {
                desc.push_str("ERROR: ");
                desc.push_str(&e);
                Ok(vec![desc])
            }
        }
    } else if let Some(payload) = custom_payload {
        let mut subsets: std::collections::HashMap<String, Vec<&dyn TemplateEntity>> =
            std::collections::HashMap::new();
        let mut viewer_ids = vec!["bystander_1".to_string()];

        for entity in &payload.entities {
            let is_viewer_capable = payload
                .subsets
                .get(&entity.subset)
                .is_none_or(|s| s.viewer_capable);

            if is_viewer_capable {
                viewer_ids.push(entity.id.clone());
            }

            subsets
                .entry(entity.subset.clone())
                .or_default()
                .push(entity as &dyn TemplateEntity);
        }

        let stances = vec![
            ActorStance::FirstPerson,
            ActorStance::SecondPerson,
            ActorStance::ThirdPerson,
        ];
        let tenses = vec![Tense::Present, Tense::Past, Tense::Future];

        generate_template_permutations(
            template,
            &viewer_ids,
            &stances,
            &tenses,
            &subsets,
            bindings,
            lookahead,
        )
    } else {
        test_template_with_standard_entities(template, bindings, lookahead)
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();

    let args: Vec<String> = env::args().collect();

    let mut input_path = String::new();
    let mut output_path = None;
    let mut entities_path = None;
    let mut context_path = None;
    let mut interactive = false;
    let mut lookahead = false;
    let mut cli_bindings = std::collections::HashMap::new();

    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--entities" {
            entities_path = iter.next().cloned();
        } else if arg == "--context" || arg == "-c" {
            context_path = iter.next().cloned();
        } else if arg == "--interactive" || arg == "-i" {
            interactive = true;
        } else if arg == "--lookahead" || arg == "-l" {
            lookahead = true;
        } else if arg == "--bind" || arg == "-b" {
            if let Some(bind_str) = iter.next() {
                if let Some((k, v)) = bind_str.split_once('=') {
                    cli_bindings.insert(k.to_string(), v.to_string());
                } else {
                    eprintln!("Invalid bind format. Expected key=subset (e.g., weapon=objects)");
                    process::exit(1);
                }
            }
        } else if input_path.is_empty() {
            input_path = arg.clone();
        } else if output_path.is_none() {
            output_path = Some(arg.clone());
        } else {
            eprintln!("Unexpected argument: {}", arg);
            print_usage_and_exit(&args[0]);
        }
    }

    if input_path.is_empty() {
        interactive = true;
    }

    let mut custom_payload = entities_path.map(|path| {
        let entities_text = fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("Error reading entities file '{}': {}", path, e);
            process::exit(1);
        });

        match serde_json::from_str::<EntitiesPayload>(&entities_text) {
            Ok(payload) => payload,
            Err(err_payload) => {
                match serde_json::from_str::<Vec<DebugEntity>>(&entities_text) {
                    Ok(entities) => EntitiesPayload {
                        subsets: std::collections::HashMap::new(),
                        entities,
                    },
                    Err(err_vec) => {
                        eprintln!("Error parsing entities JSON. Attempted two formats:\n1. As `EntitiesPayload` (with subsets): {}\n2. As a flat `Vec<DebugEntity>`: {}", err_payload, err_vec);
                        process::exit(1);
                    }
                }
            }
        }
    });

    let specific_context = context_path.map(|path| {
        let context_text = fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("Error reading context file '{}': {}", path, e);
            process::exit(1);
        });

        match serde_json::from_str::<TestContextDef>(&context_text) {
            Ok(context) => context,
            Err(e) => {
                eprintln!("Error parsing context JSON from '{}':\n{}", path, e);
                process::exit(1);
            }
        }
    });

    if interactive && custom_payload.is_none() {
        let mut subsets = std::collections::HashMap::new();
        subsets.insert(
            "actors".to_string(),
            mud_perspective::debug::SubsetConfig {
                viewer_capable: true,
            },
        );
        subsets.insert(
            "objects".to_string(),
            mud_perspective::debug::SubsetConfig {
                viewer_capable: false,
            },
        );
        custom_payload = Some(EntitiesPayload {
            subsets,
            entities: mud_perspective::debug::standard_test_entities(),
        });
    }

    // Apply sensible default bindings for common keys if they haven't been explicitly bound
    // and the target subsets actually exist in the loaded payload.
    let common_defaults = [
        ("source", "actors"),
        ("target", "actors"),
        ("weapon", "objects"),
        ("item", "objects"),
    ];

    for (key, subset) in common_defaults {
        if !cli_bindings.contains_key(key) {
            let is_valid = if let Some(payload) = custom_payload.as_ref() {
                payload.subsets.contains_key(subset)
                    || payload.entities.iter().any(|e| e.subset == subset)
            } else {
                true
            };
            if is_valid {
                cli_bindings.insert(key.to_string(), subset.to_string());
            }
        }
    }

    if interactive {
        let print_help = || {
            println!("Commands:");
            println!("  bind <key>=<subset> : Bind a template key to an entity subset");
            println!("  unbind <key>        : Remove a binding");
            println!("  bindings            : List current bindings");
            println!("  sets                : List all subsets");
            println!("  set <name>          : List entities in a subset");
            println!("  add <subset>,<id>,<name>,<gender>,<pl>,<prop> : Add an entity");
            println!("  remove <id>         : Remove an entity");
            println!("  help                : Show this help message");
            println!("  exit / quit         : Exit the tester");
        };

        println!("MUD Perspective Interactive Template Tester");
        println!("Type a template and press Enter to evaluate it.");
        print_help();

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut persistent_bindings = cli_bindings.clone();

        loop {
            print!("> ");
            if let Err(e) = stdout.flush() {
                eprintln!("Error flushing stdout: {}", e);
            }

            let mut input = String::new();
            if stdin.read_line(&mut input).is_err() {
                break;
            }

            let input = input.trim();
            if input.is_empty() {
                continue;
            }
            if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                break;
            }

            if input.eq_ignore_ascii_case("help") {
                print_help();
                continue;
            }

            if input.eq_ignore_ascii_case("bindings") {
                if persistent_bindings.is_empty() {
                    println!("No active bindings.");
                } else {
                    println!("Current bindings:");
                    for (k, v) in &persistent_bindings {
                        println!("  {} = {}", k, v);
                    }
                }
                continue;
            }

            if let Some(rest) = input.strip_prefix("bind ") {
                let rest = rest.trim();
                if let Some((k, v)) = rest.split_once('=') {
                    let subset = v.trim();
                    let is_valid = if let Some(payload) = custom_payload.as_ref() {
                        payload.subsets.contains_key(subset)
                            || payload.entities.iter().any(|e| e.subset == subset)
                    } else {
                        true
                    };

                    if is_valid {
                        persistent_bindings.insert(k.trim().to_string(), subset.to_string());
                        println!("Bound '{}' to '{}'", k.trim(), subset);
                    } else {
                        println!("Error: Subset '{}' does not exist.", subset);
                    }
                } else {
                    println!(
                        "Invalid bind format. Use: bind key=subset (e.g., bind weapon=objects)"
                    );
                }
                continue;
            }

            if let Some(rest) = input.strip_prefix("unbind ") {
                let key = rest.trim();
                if persistent_bindings.remove(key).is_some() {
                    println!("Unbound '{}'", key);
                } else {
                    println!("Key '{}' was not bound.", key);
                }
                continue;
            }

            if input.eq_ignore_ascii_case("sets") {
                if let Some(payload) = custom_payload.as_ref() {
                    let mut all_subsets = std::collections::HashSet::new();
                    for key in payload.subsets.keys() {
                        all_subsets.insert(key.clone());
                    }
                    for entity in &payload.entities {
                        all_subsets.insert(entity.subset.clone());
                    }
                    if all_subsets.is_empty() {
                        println!("No subsets defined.");
                    } else {
                        println!("Subsets:");
                        let mut sorted: Vec<_> = all_subsets.iter().collect();
                        sorted.sort();
                        for subset in sorted {
                            println!("  {}", subset);
                        }
                    }
                } else {
                    println!("No payload loaded.");
                }
                continue;
            }

            if let Some(rest) = input.strip_prefix("set ") {
                let subset_name = rest.trim();
                if let Some(payload) = custom_payload.as_ref() {
                    let matching: Vec<_> = payload
                        .entities
                        .iter()
                        .filter(|e| e.subset == subset_name)
                        .collect();
                    if matching.is_empty() {
                        println!("No entities in subset '{}'.", subset_name);
                    } else {
                        println!("Entities in subset '{}':", subset_name);
                        for entity in matching {
                            println!(
                                "  id: {}, name: {}, gender: {:?}, plural: {}, proper: {}",
                                entity.id,
                                entity.name,
                                entity.gender,
                                entity.is_plural,
                                entity.is_proper_noun
                            );
                        }
                    }
                } else {
                    println!("No payload loaded.");
                }
                continue;
            }

            if let Some(rest) = input.strip_prefix("add ") {
                let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
                if parts.len() == 6 {
                    let subset = parts[0].to_string();
                    let id = parts[1].to_string();
                    let name = parts[2].to_string();
                    let gender_str = parts[3].to_lowercase();
                    let is_plural = parts[4].eq_ignore_ascii_case("true");
                    let is_proper_noun = parts[5].eq_ignore_ascii_case("true");

                    let gender = match gender_str.as_str() {
                        "male" => Gender::Male,
                        "female" => Gender::Female,
                        "neutral" => Gender::Neutral,
                        "plural" => Gender::Plural,
                        _ => {
                            println!("Invalid gender. Use: male, female, neutral, or plural");
                            continue;
                        }
                    };

                    if let Some(payload) = custom_payload.as_mut() {
                        payload.entities.push(DebugEntity {
                            id,
                            name,
                            gender,
                            is_plural,
                            is_proper_noun,
                            subset,
                        });
                        println!("Entity added.");
                    }
                } else {
                    println!("Invalid add format. Use: add subset,id,name,gender,plural,proper");
                }
                continue;
            }

            if let Some(rest) = input.strip_prefix("remove ") {
                let id = rest.trim();
                if let Some(payload) = custom_payload.as_mut() {
                    let initial_len = payload.entities.len();
                    payload.entities.retain(|e| e.id != id);
                    if payload.entities.len() < initial_len {
                        println!("Entity '{}' removed.", id);
                    } else {
                        println!("Entity '{}' not found.", id);
                    }
                }
                continue;
            }

            if !input.contains('{') && !input.contains('[') {
                println!(
                    "Input contains no template tags ('{{' or '['). If you meant to run a command, type 'help'."
                );
                continue;
            }

            let template = match Template::compile(input) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error compiling template:\n{}", e);
                    continue;
                }
            };

            for key in &template.template_keys {
                if !persistent_bindings.contains_key(key) {
                    loop {
                        print!("Assign subset for tag '{{{}}}' [actors]: ", key);
                        if let Err(e) = stdout.flush() {
                            eprintln!("Error flushing stdout: {}", e);
                        }
                        let mut input = String::new();
                        if stdin.read_line(&mut input).is_err() {
                            break;
                        }
                        let input = input.trim();
                        let subset = if input.is_empty() {
                            "actors".to_string()
                        } else {
                            input.to_string()
                        };

                        let is_valid = if let Some(payload) = custom_payload.as_ref() {
                            payload.subsets.contains_key(&subset)
                                || payload.entities.iter().any(|e| e.subset == subset)
                        } else {
                            true
                        };

                        if is_valid {
                            persistent_bindings.insert(key.clone(), subset);
                            break;
                        } else {
                            println!("Error: Subset '{}' does not exist. Try again.", subset);
                        }
                    }
                }
            }

            match evaluate_template(
                &template,
                custom_payload.as_ref(),
                specific_context.as_ref(),
                &persistent_bindings,
                lookahead,
            ) {
                Ok(permutations) => {
                    for p in permutations {
                        println!("{}", p);
                    }
                }
                Err(e) => {
                    eprintln!("Error generating permutations:\n{}", e);
                }
            }
        }
    } else {
        let template_text = fs::read_to_string(&input_path).unwrap_or_else(|e| {
            eprintln!("Error reading input file '{}': {}", input_path, e);
            process::exit(1);
        });

        let template = Template::compile(&template_text).unwrap_or_else(|e| {
            eprintln!("Error compiling template:\n{}", e);
            process::exit(1);
        });

        let permutations = evaluate_template(
            &template,
            custom_payload.as_ref(),
            specific_context.as_ref(),
            &cli_bindings,
            lookahead,
        )
        .unwrap_or_else(|e| {
            eprintln!("Error generating permutations:\n{}", e);
            process::exit(1);
        });

        if let Some(output_path) = output_path {
            let mut file = fs::File::create(&output_path).unwrap_or_else(|e| {
                eprintln!("Error creating output file '{}': {}", output_path, e);
                process::exit(1);
            });

            for p in &permutations {
                if let Err(e) = writeln!(file, "{}", p) {
                    eprintln!("Error writing to output file: {}", e);
                    process::exit(1);
                }
            }
            println!(
                "Successfully wrote {} permutations to '{}'",
                permutations.len(),
                output_path
            );
        } else {
            for p in permutations {
                println!("{}", p);
            }
        }
    }
}
