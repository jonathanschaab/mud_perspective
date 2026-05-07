use mud_perspective::debug::{
    DebugEntity, EntitiesPayload, generate_template_permutations,
    test_template_with_standard_entities,
};
use mud_perspective::engine::{PerspectiveEngine, Template};
use mud_perspective::models::{ActorStance, Gender, RenderContext, TemplateEntity, Tense};
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process;

#[derive(serde::Deserialize)]
struct TestContextDef {
    viewer_id: String,
    stance: ActorStance,
    tense: Tense,
    entities: std::collections::HashMap<String, DebugEntity>,
}

fn get_usage(bin_name: &str) -> String {
    format!(
        "Usage: {} [input_template_file] [output_file] [--entities <json_file>] [--bind <key=subset>] [--interactive | -i] [--lookahead | -l]\n\
         [input_template_file] : Path to a text file containing the raw template.\n\
         [output_file]         : (Optional) Path to save the generated permutations.\n\
                                 If omitted, results are printed to standard output.\n\
         --entities <json_file>: (Optional) Path to a JSON file containing an array of custom entities.\n\
         --bind <key=subset>   : (Optional) Bind a template key to a specific entity subset (e.g. --bind weapon=objects).\n\
         --interactive, -i     : Start in interactive mode. This is the default if no input file is provided.\n\
         --lookahead, -l       : Enable lookahead (AST Pre-Pass) to test omniscient collision resolution.",
        bin_name
    )
}

#[derive(Debug, PartialEq)]
struct CliConfig {
    input_path: String,
    output_path: Option<String>,
    entities_path: Option<String>,
    context_path: Option<String>,
    interactive: bool,
    lookahead: bool,
    cli_bindings: std::collections::HashMap<String, String>,
}

fn parse_args(args: &[String]) -> Result<CliConfig, String> {
    let mut config = CliConfig {
        input_path: String::new(),
        output_path: None,
        entities_path: None,
        context_path: None,
        interactive: false,
        lookahead: false,
        cli_bindings: std::collections::HashMap::new(),
    };

    let bin_name = args
        .first()
        .map(String::as_str)
        .unwrap_or("mud_template_tester");
    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        if arg == "--entities" {
            config.entities_path = iter.next().cloned();
        } else if arg == "--context" || arg == "-c" {
            config.context_path = iter.next().cloned();
        } else if arg == "--interactive" || arg == "-i" {
            config.interactive = true;
        } else if arg == "--lookahead" || arg == "-l" {
            config.lookahead = true;
        } else if arg == "--bind" || arg == "-b" {
            if let Some(bind_str) = iter.next() {
                if let Some((k, v)) = bind_str.split_once('=') {
                    config.cli_bindings.insert(k.to_string(), v.to_string());
                } else {
                    return Err(
                        "Invalid bind format. Expected key=subset (e.g., weapon=objects)"
                            .to_string(),
                    );
                }
            }
        } else if config.input_path.is_empty() {
            config.input_path = arg.clone();
        } else if config.output_path.is_none() {
            config.output_path = Some(arg.clone());
        } else {
            return Err(format!(
                "Unexpected argument: {}\n\n{}",
                arg,
                get_usage(bin_name)
            ));
        }
    }

    if config.input_path.is_empty() {
        config.interactive = true;
    }

    Ok(config)
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

        let res = PerspectiveEngine::render(template, &ctx)?;
        desc.push_str(&res);
        Ok(vec![desc])
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

fn main() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();

    let args: Vec<String> = env::args().collect();

    let config = match parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    let input_path = config.input_path;
    let output_path = config.output_path;
    let entities_path = config.entities_path;
    let context_path = config.context_path;
    let interactive = config.interactive;
    let lookahead = config.lookahead;
    let mut cli_bindings = config.cli_bindings;

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
            let is_valid = custom_payload.as_ref().is_none_or(|p| p.has_subset(subset));
            if is_valid {
                cli_bindings.insert(key.to_string(), subset.to_string());
            }
        }
    }

    if interactive {
        println!("MUD Perspective Interactive Template Tester");
        println!("Type a template and press Enter to evaluate it.");

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut stdin_lock = stdin.lock();

        let _ = print_interactive_help(&mut stdout);
        let mut persistent_bindings = cli_bindings.clone();
        let mut cached_subsets: Option<Vec<String>> = None;

        loop {
            print!("> ");
            if let Err(e) = stdout.flush() {
                eprintln!("Error flushing stdout: {}", e);
            }

            let mut input = String::new();
            if stdin_lock.read_line(&mut input).is_err() {
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
                let _ = print_interactive_help(&mut stdout);
                continue;
            }

            if input.eq_ignore_ascii_case("bindings") {
                let _ = handle_bindings(&persistent_bindings, &mut stdout);
                continue;
            }

            if let Some(rest) = input.strip_prefix("bind ") {
                let _ = handle_bind(
                    rest,
                    &mut persistent_bindings,
                    custom_payload.as_ref(),
                    &mut stdout,
                );
                continue;
            }

            if let Some(rest) = input.strip_prefix("unbind ") {
                let _ = handle_unbind(rest, &mut persistent_bindings, &mut stdout);
                continue;
            }

            if input.eq_ignore_ascii_case("sets") {
                let _ = handle_sets(custom_payload.as_ref(), &mut cached_subsets, &mut stdout);
                continue;
            }

            if let Some(rest) = input.strip_prefix("set ") {
                let _ = handle_set(rest, custom_payload.as_ref(), &mut stdout);
                continue;
            }

            if let Some(rest) = input.strip_prefix("add ") {
                let _ = handle_add(rest, &mut custom_payload, &mut cached_subsets, &mut stdout);
                continue;
            }

            if let Some(rest) = input.strip_prefix("remove ") {
                let _ = handle_remove(rest, &mut custom_payload, &mut cached_subsets, &mut stdout);
                continue;
            }

            let _ = handle_template_evaluation(
                input,
                &mut persistent_bindings,
                custom_payload.as_ref(),
                specific_context.as_ref(),
                lookahead,
                &mut stdout,
                &mut stdin_lock,
            );
        }
    } else {
        let template_text = fs::read_to_string(&input_path)
            .map_err(|e| format!("Error reading input file '{}': {}", input_path, e))?;

        let template = Template::compile(&template_text)
            .map_err(|e| format!("Error compiling template:\n{}", e))?;

        let permutations = evaluate_template(
            &template,
            custom_payload.as_ref(),
            specific_context.as_ref(),
            &cli_bindings,
            lookahead,
        )
        .map_err(|e| format!("Error generating permutations:\n{}", e))?;

        if let Some(output_path) = output_path {
            let mut file = fs::File::create(&output_path)
                .map_err(|e| format!("Error creating output file '{}': {}", output_path, e))?;

            for p in &permutations {
                writeln!(file, "{}", p)
                    .map_err(|e| format!("Error writing to output file: {}", e))?;
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
    Ok(())
}

fn print_interactive_help(writer: &mut dyn Write) -> io::Result<()> {
    writeln!(writer, "Commands:")?;
    writeln!(
        writer,
        "  bind <key>=<subset> : Bind a template key to an entity subset"
    )?;
    writeln!(writer, "  unbind <key>        : Remove a binding")?;
    writeln!(writer, "  bindings            : List current bindings")?;
    writeln!(writer, "  sets                : List all subsets")?;
    writeln!(writer, "  set <name>          : List entities in a subset")?;
    writeln!(
        writer,
        "  add <subset>,<id>,<name>,<gender>,<pl>,<prop> : Add an entity"
    )?;
    writeln!(writer, "  remove <id>         : Remove an entity")?;
    writeln!(writer, "  help                : Show this help message")?;
    writeln!(writer, "  exit / quit         : Exit the tester")?;
    Ok(())
}

fn handle_bindings(
    bindings: &std::collections::HashMap<String, String>,
    writer: &mut dyn Write,
) -> io::Result<()> {
    if bindings.is_empty() {
        writeln!(writer, "No active bindings.")?;
    } else {
        writeln!(writer, "Current bindings:")?;
        for (k, v) in bindings {
            writeln!(writer, "  {} = {}", k, v)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use mud_perspective::debug::SubsetConfig;
    use mud_perspective::models::Gender;

    #[test]
    fn test_handle_bind_valid() {
        let mut bindings = std::collections::HashMap::new();
        let mut subsets = std::collections::HashMap::new();
        subsets.insert(
            "objects".to_string(),
            SubsetConfig {
                viewer_capable: false,
            },
        );
        let payload = EntitiesPayload {
            subsets,
            entities: vec![],
        };

        let mut output = Vec::new();
        handle_bind("weapon=objects", &mut bindings, Some(&payload), &mut output)
            .expect("Write failed");
        assert_eq!(bindings.get("weapon").map(String::as_str), Some("objects"));
    }

    #[test]
    fn test_handle_bind_invalid() {
        let mut bindings = std::collections::HashMap::new();
        let mut subsets = std::collections::HashMap::new();
        subsets.insert(
            "actors".to_string(),
            SubsetConfig {
                viewer_capable: true,
            },
        );
        let payload = EntitiesPayload {
            subsets,
            entities: vec![],
        };

        let mut output = Vec::new();
        handle_bind("weapon=objects", &mut bindings, Some(&payload), &mut output)
            .expect("Write failed");
        assert!(
            bindings.is_empty(),
            "Binding should not be added if the subset does not exist in the payload"
        );
    }

    #[test]
    fn test_handle_unbind() {
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("weapon".to_string(), "objects".to_string());

        let mut output = Vec::new();
        handle_unbind("weapon", &mut bindings, &mut output).expect("Write failed");
        assert!(
            bindings.is_empty(),
            "Binding should be successfully removed"
        );
    }

    #[test]
    fn test_handle_add_and_remove() {
        let mut payload = Some(EntitiesPayload {
            subsets: std::collections::HashMap::new(),
            entities: vec![],
        });
        let mut cached_subsets = Some(vec!["actors".to_string()]);

        // 1. Test Add
        let mut output_add = Vec::new();
        handle_add(
            "actors, char_1, Aldran, male, false, true",
            &mut payload,
            &mut cached_subsets,
            &mut output_add,
        )
        .expect("Write failed");

        let entities = &payload.as_ref().expect("Payload should exist").entities;
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].id, "char_1");
        assert_eq!(entities[0].name, "Aldran");
        assert_eq!(entities[0].gender, Gender::Male);
        assert_eq!(entities[0].is_plural, false);
        assert_eq!(entities[0].is_proper_noun, true);
        assert_eq!(entities[0].subset, "actors");

        assert!(
            cached_subsets.is_none(),
            "Cached subsets should be invalidated on add"
        );

        // 2. Test Remove
        let mut cached_subsets_again = Some(vec!["actors".to_string()]);
        let mut output_rm = Vec::new();
        handle_remove(
            "char_1",
            &mut payload,
            &mut cached_subsets_again,
            &mut output_rm,
        )
        .expect("Write failed");

        let entities_after = &payload.as_ref().expect("Payload should exist").entities;
        assert!(entities_after.is_empty(), "Entity should be removed");
        assert!(
            cached_subsets_again.is_none(),
            "Cached subsets should be invalidated on remove"
        );
    }

    #[test]
    fn test_parse_args_basic() {
        let args = vec![
            "tester".to_string(),
            "input.txt".to_string(),
            "output.txt".to_string(),
        ];
        let config = parse_args(&args).expect("Failed to parse args");
        assert_eq!(config.input_path, "input.txt");
        assert_eq!(config.output_path, Some("output.txt".to_string()));
        assert!(!config.interactive);
    }

    #[test]
    fn test_parse_args_interactive() {
        let args = vec!["tester".to_string(), "-i".to_string()];
        let config = parse_args(&args).expect("Failed to parse args");
        assert!(config.interactive);
        assert!(config.input_path.is_empty());
    }

    #[test]
    fn test_parse_args_bindings() {
        let args = vec![
            "tester".to_string(),
            "--bind".to_string(),
            "weapon=objects".to_string(),
        ];
        let config = parse_args(&args).expect("Failed to parse args");
        assert_eq!(
            config.cli_bindings.get("weapon").map(String::as_str),
            Some("objects")
        );
    }

    #[test]
    fn test_parse_args_invalid_argument() {
        let args = vec![
            "tester".to_string(),
            "input.txt".to_string(),
            "output.txt".to_string(),
            "extra.txt".to_string(),
        ];
        let err = parse_args(&args).expect_err("Should fail on extra argument");
        assert!(err.contains("Unexpected argument: extra.txt"));
    }
}

fn handle_bind(
    args: &str,
    bindings: &mut std::collections::HashMap<String, String>,
    payload: Option<&EntitiesPayload>,
    writer: &mut dyn Write,
) -> io::Result<()> {
    let args = args.trim();
    if let Some((k, v)) = args.split_once('=') {
        let subset = v.trim();
        let is_valid = payload.is_none_or(|p| p.has_subset(subset));

        if is_valid {
            bindings.insert(k.trim().to_string(), subset.to_string());
            writeln!(writer, "Bound '{}' to '{}'", k.trim(), subset)?;
        } else {
            writeln!(writer, "Error: Subset '{}' does not exist.", subset)?;
        }
    } else {
        writeln!(
            writer,
            "Invalid bind format. Use: bind key=subset (e.g., bind weapon=objects)"
        )?;
    }
    Ok(())
}

fn handle_unbind(
    args: &str,
    bindings: &mut std::collections::HashMap<String, String>,
    writer: &mut dyn Write,
) -> io::Result<()> {
    let key = args.trim();
    if bindings.remove(key).is_some() {
        writeln!(writer, "Unbound '{}'", key)?;
    } else {
        writeln!(writer, "Key '{}' was not bound.", key)?;
    }
    Ok(())
}

fn handle_sets(
    payload: Option<&EntitiesPayload>,
    cached_subsets: &mut Option<Vec<String>>,
    writer: &mut dyn Write,
) -> io::Result<()> {
    if let Some(payload) = payload {
        let sorted_subsets = cached_subsets.get_or_insert_with(|| {
            let mut all_subsets = std::collections::HashSet::new();
            for key in payload.subsets.keys() {
                all_subsets.insert(key.clone());
            }
            for entity in &payload.entities {
                all_subsets.insert(entity.subset.clone());
            }
            let mut sorted: Vec<_> = all_subsets.into_iter().collect();
            sorted.sort();
            sorted
        });

        if sorted_subsets.is_empty() {
            writeln!(writer, "No subsets defined.")?;
        } else {
            writeln!(writer, "Subsets:")?;
            for subset in sorted_subsets {
                writeln!(writer, "  {}", subset)?;
            }
        }
    } else {
        writeln!(writer, "No payload loaded.")?;
    }
    Ok(())
}

fn handle_set(
    args: &str,
    payload: Option<&EntitiesPayload>,
    writer: &mut dyn Write,
) -> io::Result<()> {
    let subset_name = args.trim();
    if let Some(payload) = payload {
        let matching: Vec<_> = payload
            .entities
            .iter()
            .filter(|e| e.subset == subset_name)
            .collect();
        if matching.is_empty() {
            writeln!(writer, "No entities in subset '{}'.", subset_name)?;
        } else {
            writeln!(writer, "Entities in subset '{}':", subset_name)?;
            for entity in matching {
                writeln!(
                    writer,
                    "  id: {}, name: {}, gender: {:?}, plural: {}, proper: {}",
                    entity.id, entity.name, entity.gender, entity.is_plural, entity.is_proper_noun
                )?;
            }
        }
    } else {
        writeln!(writer, "No payload loaded.")?;
    }
    Ok(())
}

fn handle_add(
    args: &str,
    payload: &mut Option<EntitiesPayload>,
    cached_subsets: &mut Option<Vec<String>>,
    writer: &mut dyn Write,
) -> io::Result<()> {
    let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
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
                writeln!(
                    writer,
                    "Invalid gender. Use: male, female, neutral, or plural"
                )?;
                return Ok(());
            }
        };

        if let Some(payload) = payload.as_mut() {
            payload.entities.push(DebugEntity {
                id,
                name,
                gender,
                is_plural,
                is_proper_noun,
                subset,
            });
            *cached_subsets = None;
            writeln!(writer, "Entity added.")?;
        }
    } else {
        writeln!(
            writer,
            "Invalid add format. Use: add subset,id,name,gender,plural,proper"
        )?;
    }
    Ok(())
}

fn handle_remove(
    args: &str,
    payload: &mut Option<EntitiesPayload>,
    cached_subsets: &mut Option<Vec<String>>,
    writer: &mut dyn Write,
) -> io::Result<()> {
    let id = args.trim();
    if let Some(payload) = payload.as_mut() {
        let initial_len = payload.entities.len();
        payload.entities.retain(|e| e.id != id);
        if payload.entities.len() < initial_len {
            *cached_subsets = None;
            writeln!(writer, "Entity '{}' removed.", id)?;
        } else {
            writeln!(writer, "Entity '{}' not found.", id)?;
        }
    }
    Ok(())
}

fn handle_template_evaluation(
    input: &str,
    persistent_bindings: &mut std::collections::HashMap<String, String>,
    custom_payload: Option<&EntitiesPayload>,
    specific_context: Option<&TestContextDef>,
    lookahead: bool,
    writer: &mut dyn Write,
    reader: &mut dyn BufRead,
) -> io::Result<()> {
    if !input.contains('{') && !input.contains('[') {
        writeln!(
            writer,
            "Input contains no template tags ('{{' or '['). If you meant to run a command, type 'help'."
        )?;
        return Ok(());
    }

    let template = match Template::compile(input) {
        Ok(t) => t,
        Err(e) => {
            writeln!(writer, "Error compiling template:\n{}", e)?;
            return Ok(());
        }
    };

    for key in &template.template_keys {
        if !persistent_bindings.contains_key(key) {
            loop {
                write!(writer, "Assign subset for tag '{{{}}}' [actors]: ", key)?;
                let _ = writer.flush();

                let mut assign_input = String::new();
                if reader.read_line(&mut assign_input).is_err() {
                    break;
                }
                let assign_input = assign_input.trim();
                let subset = if assign_input.is_empty() {
                    "actors".to_string()
                } else {
                    assign_input.to_string()
                };

                let is_valid = custom_payload.is_none_or(|p| p.has_subset(&subset));

                if is_valid {
                    persistent_bindings.insert(key.clone(), subset);
                    break;
                } else {
                    writeln!(
                        writer,
                        "Error: Subset '{}' does not exist. Try again.",
                        subset
                    )?;
                }
            }
        }
    }

    match evaluate_template(
        &template,
        custom_payload,
        specific_context,
        persistent_bindings,
        lookahead,
    ) {
        Ok(permutations) => {
            for p in permutations {
                writeln!(writer, "{}", p)?;
            }
        }
        Err(e) => {
            writeln!(writer, "Error generating permutations:\n{}", e)?;
        }
    }
    Ok(())
}
