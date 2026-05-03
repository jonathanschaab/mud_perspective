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

// Optional: Expose nested entities like body parts, targets, or equipment
fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
    match property_name {
        "weapon" => self.equipped_weapon.as_ref().map(|w| w as &dyn TemplateEntity),
        _ => None,
    }
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
let template = cache.get_or_compile("{the:source} [source:watch] as {the:target} [target:approach].")?;

let player = Character { /*... */ };  
let goblin = Character { /*... */ };

// Actor Stance (The player is the viewer)  
let output_actor = render_msg!("char_1", &template, "source" => &player, "target" => &goblin)?;  
// Output: "You watch as the goblin approaches."

// Director Stance (A third-party bystander is the viewer)  
let output_director = render_msg!("char_3", &template, "source" => &player, "target" => &goblin)?;  
// Output: "Aldran watches as the goblin approaches."
```

### **2.1 Configuring Actor Stance**

By default, the engine uses the Second Person for the active viewer ("You walk forward"). You can change this behavior at render time by configuring the `RenderContext` with an `ActorStance`.

```rust
use mud_perspective::models::{ActorStance, RenderContext};

// Second Person (Default)
let ctx_second = RenderContext::new("char_1").with_entity("source", &player);
// Output: "You walk forward."

// First Person
let ctx_first = RenderContext::new("char_1")
    .with_stance(ActorStance::FirstPerson)
    .with_entity("source", &player);
// Output: "I walk forward."

// Third Person
let ctx_third = RenderContext::new("char_1")
    .with_stance(ActorStance::ThirdPerson)
    .with_entity("source", &player);
// Output: "Aldran walks forward."
```

### **2.2 Custom Runtime Verbs**

The engine also allows developers to expand its vocabulary at runtime by injecting custom irregular verbs or dialect-specific forms. This makes it easy to add new verbs dynamically without modifying the static core map.

```rust
use mud_perspective::grammar::{add_irregular_verb, remove_irregular_verb, clear_irregular_verbs};

if let Err(e) = add_irregular_verb("teleport", "teleports") {
    eprintln!("Failed to add custom verb: {e}");
}
remove_irregular_verb("teleport");
clear_irregular_verbs();
```

### 3. Handling Groups and Swarms

The library provides a built-in `GroupEntity` to easily represent dynamic groups of characters or objects. It automatically handles Oxford comma formatting, injects "you" if the viewer is in the group, and evaluates as plural so verbs and pronouns automatically conjugate correctly ("attack" instead of "attacks", "themselves", etc.).

Furthermore, the engine implements grammatical rules for mixed-person groups. If a group evaluates in the First Person alongside other entities, it decomposes and orders the pronouns (e.g., `"You, the goblin, and I"`). It also distributes joint possessive suffixes across the list if a possessive pronoun is involved (e.g., `"your, the goblin's, and my gold"`), and collapses mixed-group objective pronouns into `"us"`.

```rust
use mud_perspective::models::GroupEntity;

let party = GroupEntity { members: vec![&player, &ally] };
let template = cache.get_or_compile("{source} [source:open] the door.")?;

// Player's Perspective (Second Person): "You and Bob open the door."
// Player's Perspective (First Person): "Bob and I open the door."
// Bystander's Perspective: "Aldran and Bob open the door."
```

### **4. Syntax Reference**

* **Entities:** {key} inserts the entity's display name. Use {Key} to force capitalization mid-sentence. Prepend a plus (`{+key}`) to force the engine to render the character's 3rd-person name (Director Stance) even if the viewer is that character.
* **Nested Properties:** Use dot-notation (e.g., `{source.weapon}`) to dynamically traverse nested entities. The parent entity must implement `get_property`. Nested properties inherit all formatting rules for articles, pronouns, and possessive suffixes.
* **Possessive Nouns:** Append `'s` to any entity tag (e.g., `{source's}` or `{the:source's}`) to dynamically generate the correct possessive noun suffix. If the entity is the viewer, it automatically renders as "your" or "my". Plural entities ending in "s" (like "wolves") will correctly render with just an apostrophe ("wolves'"). Group Entities distribute possessives across all members if mixed with a pronoun (e.g., "your and the goblin's"), or append to the final item (e.g., "Aldran and the goblin's").
* **Articles / Demonstratives:** {a:key}, {the:key}, {this:key}, or {that:key} prepends the appropriate word. Indefinite articles ("a") automatically adapt to "some" for plural entities, and demonstratives automatically adapt to plural ("these", "those"). Use {A:key}, {The:key}, etc. to force capitalization mid-sentence. These are automatically suppressed if the entity evaluates to the viewer ("you") or is flagged as a proper noun. You can force an article to render for a proper noun by prepending a plus sign (e.g., `{+this:key}`).
* **Pronouns:** {key:type}. Supported types include subj (he/she/it/they), obj (him/her/it/them), poss (his/her/their), abs_poss (his/hers/theirs), and reflex (himself/themselves). Capitalize the type (e.g., {key:Subj}) to force capitalization mid-sentence. Prepend a plus (`{+key:subj}`) to force a 3rd-person pronoun (e.g., he/she/it/they) even if the viewer is the entity. The engine features automatic Anaphora Resolution to prevent pronoun ambiguity (see Section 5).

* **Verbs:** [key:verb] explicitly binds a base verb to a subject to ensure correct conjugation (including "be" -> "is"/"are" and "was" -> "were"). Capitalize the verb (e.g., [key:Verb]) to force capitalization mid-sentence. Prepend a plus (`[+key:verb]`) to force 3rd-person conjugation. You can also bypass conjugation across all perspectives by appending a pipe and the desired form (e.g., `[key:be|be]`). You can provide multiple pipe segments to explicitly define the forms for different perspectives: `[key:freak out|freak out|freaks out]` (base/plural and 3rd-person singular) or `[key:be|am|are|is]` (1st-person singular, 2nd-person/plural, and 3rd-person singular). Phrasal verbs (e.g. `[key:pick up]`) are naturally supported; the engine dynamically isolates the first word to ensure `"pick up"` correctly conjugates to `"picks up"`.

* **Escaping:** Use a backslash (`\`) to escape special characters if you need to output literal braces or brackets (e.g., `\{`, `\}`, `\[`, `\]`). You can also escape a backslash itself (`\\`).

### **5. Smart Pronouns & Anaphora Resolution**

The engine features an Anaphora Resolution system. It allows you to write templates almost entirely with pronouns (e.g., `{target:Subj} [target:look] at {target:reflex}.`), and dynamically decides when to introduce the full name ("The goblin looks at itself.") and when to use pronouns ("It looks at itself.").

* **How it Triggers:** Whenever the engine encounters a pronoun tag, it checks the context's memory. If the entity hasn't been introduced yet, it will expand the pronoun into a fully formatted noun (including definite articles or possessive suffixes, like `the goblin's`).
* **Ambiguity Detection:** In plain terms, the engine behaves like a reader. If a pronoun is requested for an entity that isn't the active subject, the engine evaluates the "cast" of recently mentioned characters. If any other recently mentioned character shares the exact same grammatical gender and plurality (e.g., two male characters in the same sentence), the engine recognizes that outputting "He" would confuse the reader. It falls back to the full name to guarantee clarity.
* **Cross-Context Memory:** The anaphora memory lives inside the `RenderContext`. 
  * **Chaining:** You can render multiple templates in a row using the same context, and the engine will maintain narrative continuity across the templates.
  * **Game Ticks:** If your game loop spans across multiple server ticks or async events, you can extract the full narrative state using `let state = ctx.extract_anaphora()` and inject it into a brand new context later using `RenderContext::new(...).with_anaphora(state)`. This ensures ambiguity detection carries over.
* **Memory Limits (LRU):** To prevent memory from growing unbounded during extremely long, continuous encounters, the anaphora memory acts as a Least-Recently-Used (LRU) cache defaulting to 15 entities. You can configure this limit using `ctx.with_anaphora_limit(size)`.
* **Pinning & Manual Control:** In crowded scenarios, important characters might get pushed out of the LRU cache by a flurry of secondary actors. Protect them by pinning them into memory using `ctx.with_pinned_entity("key")` or `ctx.pin_anaphora("key")`. You can unpin them using `ctx.unpin_anaphora("key")`. You can also explicitly remove entities using `ctx.without_anaphora("key")` or `ctx.forget_anaphora("key")`.
* **Resetting Memory:** To manually clear the engine's memory, call `ctx.clear_anaphora()`. You should do this whenever narrative continuity is broken to prevent awkward, lingering pronoun references. Good times to call this include:
  * When a player moves to a new room or area.
  * When a significant amount of time passes between events.
  * At the start of a new, unrelated combat round or distinct paragraph.
  * *Why?* If you don't clear the memory between independent events, a template might output "He arrives." instead of "Aldran arrives." just because Aldran was the active subject of a completely unrelated event 5 minutes ago!

#### **Example: Multi-Sentence Combat Log**

Using pronouns and active subject tracking allows builders to write multi-sentence descriptions that adapt to any combination of actors.

```rust
let template = cache.get_or_compile(
    "{source} [source:kick] {the:target} in the chest. {target:Subj} [target:stumble] backward, and {source:subj} [source:press] the advantage!"
)?;

// If Aldran kicks a goblin (Unambiguous pronouns):
// "Aldran kicks the goblin in the chest. It stumbles backward, and he presses the advantage!"

// If the viewer is Aldran (Actor stance takes over):
// "You kick the goblin in the chest. It stumbles backward, and you press the advantage!"

// If Bob (Male) kicks Aldran (Male) -> Ambiguity Resolution prevents "He... he...":
// "Bob kicks Aldran in the chest. Aldran stumbles backward, and Bob presses the advantage!"
```

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
3. **Dynamic Tense Shifting:** The engine does not have a concept of narrative time. To output past-tense text, the builder must explicitly tag the past-tense form in the template (e.g., `[source:ran]`, not `[source:run]`).
4. **Past Tense Inflection by Person:** In modern English, "to be" is the only verb that changes form in the past tense based on the subject (*I was, you were*). The engine hardcodes this specific exception, but cannot dynamically handle hypothetical or custom verbs that inflect by person in the past tense.
5. **True Colliding Verbs:** The engine maps strings to strings and cannot determine the semantic context of a verb. For verbs where the past tense changes based on context (e.g., "I *bore* a sword" vs "The market *beared* it"), the static map cannot dynamically choose the correct form.
