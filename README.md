# **mud\_perspective**

mud\_perspective is a Rust library designed to handle perspective-aware text generation for Multi-User Dungeons (MUDs) and interactive fiction. In multiplayer text environments, a single event must often be described differently depending on who is observing it. This engine resolves pronouns, conjugations, and indefinite articles dynamically based on the observer's relationship to the event.

## **Goals**

The primary goal of this crate is to provide a reliable, thread-safe templating system for game servers. Its core objectives include:

* **Stance Shifting:** Automatically transitioning between "Actor Stance" (addressing the viewer in the first/second person) and "Director Stance" (addressing the viewer in the third person).

* **Subject-Verb Agreement:** Conjugating verbs correctly based on the grammatical number and person of the subject.  
* **Irregular Verb Handling:** Utilizing a static dictionary to safely conjugate common irregular and modal verbs without relying strictly on algorithmic suffixes.  
* **Epistemological Masking:** Allowing the underlying game logic to obscure entity names (e.g., using disguises or recognition systems) based on the specific observer viewing the text.
* **Memory Efficiency:** Using a highly concurrent AST cache backed by a TinyLFU eviction strategy and Cow (Clone-on-Write) strings to minimize heap allocations and lock contention during high-frequency combat loops.

## **API Usage**

### **1\. Implementing TemplateEntity**

To use the engine, your game objects must implement the TemplateEntity trait. This abstracts your database logic away from the rendering engine.

```rust
use mud_perspective::models::{TemplateEntity, Gender};

use std::borrow::Cow;

pub struct Character {

pub id: String,

pub name: String,

pub gender: Gender,

pub is_plural: bool,

pub is_proper_noun: bool,

}

impl TemplateEntity for Character {

fn contains_viewer(&self, viewer_id: &str) -> bool {

self.id == viewer_id

}

fn gender(&self) -> Gender { self.gender }

fn is_plural(&self) -> bool { self.is_plural }

fn is_proper_noun_for(&self, _viewer_id: &str) -> bool {   
    self.is_proper_noun   
}

fn display_name_for<'a>(&'a self, viewer_id: &str) -> Cow<'a, str> {  
    if self.contains_viewer(viewer_id) {
        return Cow::Borrowed("you");
    }
    // You can implement disguise logic or recognition checks here  
    Cow::Borrowed(&self.name)  
}

}
```

### 2\. Rendering Templates  
The engine provides a `render_msg!` macro to make evaluating templates against a context ergonomic for game logic.

```rust  
use mud_perspective::{render_msg, TemplateCache};

// Initialize this cache once and share it across your game state  
let cache = TemplateCache::new(1000);

// Compile the template  
let template = cache.get_or_compile("{the:source} [source:watch] as {the:target} [target:approach].").unwrap();

let player = Character { /*... */ };  
let goblin = Character { /*... */ };

// Actor Stance (The player is the viewer)  
let output_actor = render_msg!("char_1", &template, "source" => &player, "target" => &goblin).unwrap();  
// Output: "You watch as the goblin approaches."

// Director Stance (A third-party bystander is the viewer)  
let output_director = render_msg!("char_3", &template, "source" => &player, "target" => &goblin).unwrap();  
// Output: "Aldran watches as the goblin approaches."
```

### 3. Handling Groups and Swarms

The library provides a built-in `GroupEntity` to easily represent dynamic groups of characters or objects. It automatically handles Oxford comma formatting, injects "you" if the viewer is in the group, and evaluates as plural so verbs and pronouns automatically conjugate correctly ("attack" instead of "attacks", "themselves", etc.).

```rust
use mud_perspective::models::GroupEntity;

let party = GroupEntity { members: vec![&player, &ally] };
let template = cache.get_or_compile("{source} [source:open] the door.").unwrap();

// Player's Perspective: "You and Bob open the door."
// Bystander's Perspective: "Aldran and Bob open the door."
```

### **4. Syntax Reference**

* **Entities:** {key} inserts the entity's display name. Use {Key} to force capitalization mid-sentence. Prepend a plus (`{+key}`) to force the engine to render the character's 3rd-person name (Director Stance) even if the viewer is that character.
* **Articles / Demonstratives:** {a:key}, {the:key}, {this:key}, or {that:key} prepends the appropriate word. Indefinite articles ("a") automatically adapt to "some" for plural entities, and demonstratives automatically adapt to plural ("these", "those"). Use {A:key}, {The:key}, etc. to force capitalization mid-sentence. These are automatically suppressed if the entity evaluates to the viewer ("you") or is flagged as a proper noun. You can force an article to render for a proper noun by prepending a plus sign (e.g., `{+this:key}`).
* **Pronouns:** {key:type}. Supported types include subj (he/she/it/they), obj (him/her/it/them), poss (his/her/their), abs_poss (his/hers/theirs), and reflex (himself/themselves). Capitalize the type (e.g., {key:Subj}) to force capitalization mid-sentence. Prepend a plus (`{+key:subj}`) to force a 3rd-person pronoun (e.g., he/she/it/they) even if the viewer is the entity.

* **Verbs:** [key:verb] explicitly binds a base verb to a subject to ensure correct conjugation (including "be" -> "is"/"are" and "was" -> "were"). Capitalize the verb (e.g., [key:Verb]) to force capitalization mid-sentence. Prepend a plus (`[+key:verb]`) to force 3rd-person conjugation. This prevents grammatical errors during compound subjects or passive voice structures.

* **Escaping:** Use a backslash (`\`) to escape special characters if you need to output literal braces or brackets (e.g., `\{`, `\}`, `\[`, `\]`). You can also escape a backslash itself (`\\`).

## **Cargo Features**

By default, `mud_perspective` includes built-in support for parsing and safely skipping common MUD protocol tags. This ensures that the typography engine does not accidentally capitalize hidden metadata (e.g., changing `<color red>` to `<color Red>`) and that the AST compiler does not misinterpret braces or brackets hidden inside these tags.

* `ansi`: Safely skips ANSI escape sequences (e.g., `\x1b[31m`).
* `mxp`: Safely skips MUD eXtension Protocol HTML-like tags (e.g., `<SEND HREF="look">`).
* `msp`: Safely skips MUD Sound Protocol triggers (e.g., `!!SOUND(roar.wav)`).

These are enabled by default. If your MUD does not use some or all of these protocols, you can disable them to eke out extra performance during the compilation and rendering phases:

```toml
[dependencies]
mud_perspective = { version = "0.1", default-features = false, features = ["ansi"] }
```

## **Current Shortcomings**

While functional for standard MUD environments, the current architecture has several limitations that developers should be aware of:

1. **English-Only Morphology:** The verb conjugation and pronoun resolution algorithms are strictly hardcoded for English grammar. Supporting languages with complex declensions or grammatical gender agreement (e.g., Romance or Slavic languages) would require a fundamental rewrite of the grammar.rs module.  
2. **Abbreviation Boundary Detection:** The typography post-processor relies on standard Unicode sentence segmentation to capitalize the first letter of each sentence. Because it lacks a comprehensive natural language abbreviation dictionary, it may incorrectly capitalize words immediately following common abbreviations (e.g., treating the period in "Mr. Smith" as a hard sentence boundary).

3. **Static Irregular Verb Map:** The internal Perfect Hash Function (PHF) map currently only covers a curated core set of irregular and modal verbs. Verbs outside of this list fall back to algorithmic suffix rules (adding "s", "es", or "ies"), which will produce grammatically incorrect text for unmapped irregulars.  
4. **No Anaphora Resolution:** The engine evaluates syntax strictly token-by-token. It cannot contextually look back at previous sentences to determine if a noun has already been introduced, meaning it cannot automatically decide when to switch from an indefinite article ("a sword") to a definite article ("the sword") across larger paragraphs of text.
