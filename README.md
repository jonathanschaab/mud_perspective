# **mud\_perspective**

mud\_perspective is a Rust library designed to handle perspective-aware text generation for Multi-User Dungeons (MUDs) and interactive fiction. In multiplayer text environments, a single event must often be described differently depending on who is observing it. This engine resolves pronouns, conjugations, and indefinite articles dynamically based on the observer's relationship to the event.

## **Goals**

The primary goal of this crate is to provide a thread-safe templating system for game servers. Its core objectives include:

* **Stance Shifting:** Automatically transitioning between "Actor Stance" (addressing the viewer in the first/second person) and "Director Stance" (addressing the viewer in the third person).
* **Subject-Verb Agreement:** Conjugating verbs correctly based on the grammatical number and person of the subject.  
* **Irregular Verb Handling:** Utilizing a static dictionary to safely conjugate common irregular and modal verbs without relying strictly on algorithmic suffixes.  
* **Epistemological Masking:** Allowing the underlying game logic to obscure entity names (e.g., using disguises or recognition systems) based on the specific observer viewing the text.
* **Memory Efficiency:** Using a concurrent AST cache backed by a TinyLFU eviction strategy and Cow (Clone-on-Write) strings to minimize heap allocations and lock contention during high-frequency combat loops.

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
    pub equipped_weapon: Option<Weapon>,
}

// Assuming Weapon also implements TemplateEntity
pub struct Weapon {
    pub name: String,
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

    // Optional: Provide a more specific name to prevent "another" spam during collisions.
    // fn long_display_name_for<'a>(&'a self, _viewer_id: &str) -> Option<Cow<'a, str>> {
    //     // e.g., Some(Cow::Borrowed("large goblin"))
    //     None
    // }

    // Optional: Provide a collective noun for plural entities to improve ordinal phrasing.
    // fn collective_noun(&self) -> Option<&str> {
    //     if self.name == "wolves" { Some("pack") }
    //     else if self.name == "whales" { Some("pod") }
    //     else { None }
    // }

    // Optional: Provide a list of valid adjectives players can use to target this entity.
    // fn adjectives(&self) -> Option<&[&str]> {
    //     Some(&["large", "angry"])
    // }

    // Optional: Provide a list of adjective synonyms for targeting.
    // fn adjective_synonyms(&self) -> Option<&[&str]> {
    //     Some(&["big", "huge"]) // for "large"
    // }

    // Optional: Expose nested entities like body parts, targets, or equipment
    fn get_property(&self, property_name: &str) -> Option<&dyn TemplateEntity> {
        match property_name {
            "weapon" => self.equipped_weapon.as_ref().map(|w| w as &dyn TemplateEntity),
            _ => None,
        }
    }
}
```

### **2. Rendering Templates**  
The engine provides a `render_msg!` macro to evaluate templates against a context.

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

The engine allows developers to expand its vocabulary at runtime by injecting custom irregular verbs or dialect-specific forms. This allows developers to add new verbs dynamically without modifying the static core map. You can add verbs individually, or use the `register_custom_verbs!` macro to seamlessly inject multiple overrides at once (ideal for server initialization).

```rust
use mud_perspective::grammar::{add_irregular_verb, remove_irregular_verb, clear_irregular_verbs};
use mud_perspective::register_custom_verbs;

// Conveniently register multiple verbs at once
register_custom_verbs! {
    "yeet" => ("yeetses", "yeeted"),
    "make do" => ("makes do", "made do"),
};

// Or add them individually
if let Err(e) = add_irregular_verb("teleport", "teleports", "teleported") {
    eprintln!("Failed to add custom verb: {e}");
}
remove_irregular_verb("teleport");
clear_irregular_verbs();
```

### **2.3 Broadcasting Events (Cloning Contexts)**

To broadcast the same event to multiple viewers, clone a base `RenderContext` and adjust the viewer for each recipient. This avoids recompiling the template while preserving shared runtime state.

```rust
use mud_perspective::{engine::PerspectiveEngine, models::RenderContext};

let base_ctx = RenderContext::new("shared_event")
    .with_entity("source", &player)
    .with_entity("target", &goblin);

for viewer_id in room_player_ids {
    let viewer_ctx = base_ctx.clone().with_viewer(viewer_id);
    let output = PerspectiveEngine::render(&template, &viewer_ctx)?;
    // Send `output` to the viewer.
}
```

### **2.4 Dynamic Tense Shifting**

A live context can be reused for logs or memory playback by shifting tense before rendering. This is useful for a memory log, journal entry, or end-of-turn recap.

```rust
use mud_perspective::{engine::PerspectiveEngine, models::{RenderContext, Tense}};

let ctx = RenderContext::new("char_1")
    .with_entity("source", &player)
    .with_entity("target", &goblin);

let past_ctx = ctx.with_tense(Tense::Past);
let output = PerspectiveEngine::render(&template, &past_ctx)?;
// Render the same template in past tense for a memory log.
```

### **2.5 Preserving Narrative Memory Across Game Ticks**

Developers can extract the anaphora memory state to preserve pronoun continuity across asynchronous server ticks or distinct game events. This allows ambiguity detection to carry over between separate evaluation loops, even when a fresh `RenderContext` is created later.

```rust
use mud_perspective::{engine::PerspectiveEngine, models::RenderContext};

let ctx = RenderContext::new("char_1").with_entity("source", &player).with_entity("target", &goblin);
let template = cache.get_or_compile("{The:source:subj} [source:attack] {the:target:obj}. {The:target:Subj} [target:retreat].")?;

let output = PerspectiveEngine::render(&template, &ctx)?;
// Output preserves pronoun continuity for the current evaluation.

let state = ctx.extract_anaphora();

// Later, in a new tick or event, restore the same narrative memory.
let new_ctx = RenderContext::new("char_1")
    .with_entity("source", &player)
    .with_entity("target", &goblin)
    .with_anaphora(state);
```

### **2.6 Omniscient Lookahead & Disambiguation**

By default, the engine evaluates templates left-to-right sequentially, which can cause narrative "pop-in" where entities are introduced as "a goblin" and later upgraded to "another goblin" as more are discovered. For static room descriptions or pre-computed narratives, developers can opt into an AST Pre-Pass by calling `.with_lookahead(true)` on the `RenderContext`. This performs a full scan ahead of time to resolve all collisions and ordinals.

```rust
use mud_perspective::{engine::PerspectiveEngine, models::RenderContext};

let ctx = RenderContext::new("char_1")
    .with_entity("g1", &goblin1)
    .with_entity("g2", &goblin2)
    .with_entity("g3", &goblin3)
    .with_lookahead(true);

let template = cache.get_or_compile("{g1}, {g2}, and {g3} [g1:stand] here.")?;
let output = PerspectiveEngine::render(&template, &ctx)?;
// Output: "A goblin, a second goblin, and a third goblin stand here."
```

### **3. Handling Groups and Swarms**

The library provides a built-in `GroupEntity` to represent dynamic groups of characters or objects. It automatically handles Oxford comma formatting, injects "you" if the viewer is in the group, and evaluates as plural (unless the group shrinks to a single member, in which case it dynamically reverts to singular grammar) so verbs and pronouns automatically conjugate correctly.

Furthermore, the engine implements grammatical rules for mixed-person groups:
* **Pronoun Decompounding:** If a group evaluates in the First Person alongside other entities, it decomposes and orders the pronouns (e.g., `"You, the goblin, and I"`). 
* **Possessive Distribution:** It dynamically distributes joint possessive suffixes across the list if a possessive pronoun is involved (e.g., `"your, the goblin's, and my gold"`). 
* **Objective Collapsing:** Mixed-group objective pronouns collapse into `"us"`. 
* **Reflexive Overrides:** If a singular entity acts upon a group containing itself, the engine injects reflexive pronouns natively into the list (e.g., `"I slash the goblin and myself"`).
* **Nested & Empty Groups:** Group entities can safely contain other `GroupEntity` instances. The engine automatically flattens them into a single cohesive list and completely ignores any empty groups.

You can use the `^` modifier to extract an unspecified member from a group (e.g., `{^party:Subj}`). The engine will evaluate shared genders across the group or fall back to "It", and format the Oxford list using the "or" conjunction (e.g., `"You or Bob arrives"`). Additionally, you can use the `~` modifier (e.g., `{~party:Subj}`) to force the engine to permit an ambiguous "You" to refer to the whole party instead of triggering the engine's ambiguity safeguards.

```rust
use mud_perspective::models::GroupEntity;

let party = GroupEntity { members: vec![&player, &ally] };
let template = cache.get_or_compile("{source} [source:open] the door.")?;

// Player's Perspective (Second Person): "You and Bob open the door."
// Player's Perspective (First Person): "Bob and I open the door."
// Bystander's Perspective: "Aldran and Bob open the door."
```

### **4. Syntax Reference**

* **Entity Tags:** The engine uses a flexible tag syntax that scales from one to four parts: `{article:owner_and_adjectives:target:case}` (e.g., `{the:source.weapon:obj}`). It attempts to evaluate the pronoun case first (outputting "him" or "you"). If the engine detects ambiguity or it's the first mention, it falls back to the noun using your explicitly provided article (outputting "the sword"). Because most segments are optional, a single tag scales from simple noun insertions to complex, context-aware pronouns:
  * `{key}`: Inserts the entity's display name.
  * `{key:case}`: Appends a pronoun case, defaulting to an indefinite article ("a") if the engine forces a noun fallback.
  * `{article:key}`: Prepends a specific article directly to the noun.
  * `{article:adjectives:key:case}`: You can inject adjectives directly into the tag using the colon separator (e.g., `{A:glowing:sword}` or `{glowing:sword:obj}`). The engine automatically tracks these adjectives for target resolution.
  * `{article:owner's adjectives:target:case}`: Demarcates multi-word target keys from dynamically injected adjectives.
  * `{$key ?? "fallback"}`: Injects a dynamic string directly into the text. This evaluates against variables bound to the `RenderContext`, or string properties from entities (`{$source.color}`). If the variable or property is missing, the engine safely injects the fallback string! Capitalization (`{$Key}`) and all-caps (`{$KEY}`) natively format the output, including the fallback strings.
* **Comments:** `{# This is a comment #}`. Any text placed between these tags will be safely stripped by the parser at compile time and ignored. This is extremely useful for large or complex templates.
* **Conditionals:** `{% if <condition> %}...{% elif <condition> %}...{% else %}...{% endif %}`. The engine supports full boolean logic branching evaluated dynamically at render time. You can group conditions with parentheses `()` and use the operators `and` (`&&`), `or` (`||`), and `not` (`!`). Supported condition values include truthiness checks on variables or entity properties (`$var`, `source.is_bleeding`), exact string matching (`$var == "val"`, `source.color != target.color`), and numeric inequalities parsed as floats (`$health < 50.5`, `source.hp >= 10`). You must implement `check_condition` or `get_string_property` on your `TemplateEntity` to support entity property evaluations.
* **Key:** The core identifier for the entity. 
  * *Capitalization:* Capitalize the first letter (`{Key}`) to force title-casing mid-sentence. 
  * *Director Stance:* Prepend a plus (`{+key}` or `{+key:subj}`) to force the engine to render the character's 3rd-person name or pronoun even if the viewer is that character.
  * *Nested Properties:* Use dot-notation (e.g., `{source.weapon}`) to dynamically traverse nested entities. The parent entity must implement `get_property`. Nested properties inherit all formatting rules for articles, pronouns, and possessive suffixes.
* **Case:** Supported pronoun cases include `subj` (he/she/it/they), `obj` (him/her/it/them), `poss` (his/her/their), `abs_poss` (his/hers/theirs), and `reflex` (himself/themselves). 
  * *Capitalization:* Capitalize the first letter (`{key:Subj}`) to force title-casing mid-sentence.
  * *Forced Pronoun:* Prepend an exclamation mark (`{key:!subj}`) to force the pronoun to render even if the engine detects it would be ambiguous.
* **ALL CAPS:** Emphasized yelling and dramatic formatting are supported natively. If the tag's elements are written in entirely uppercase letters (e.g., `{THE:TARGET:OBJ}`, `{SOURCE}`, `[TARGET:ATTACK]`), the engine activates ALL CAPS mode. It bypasses standard title-casing and fully uppercases the output, including fallback articles, nouns, possessive suffixes, and conjugated verbs!
* **Possessive Nouns:** Append `'s` or just a trailing apostrophe (`'`) to any entity tag. This can be used standalone (`{source's}` -> "your" / "Aldran's") or combined with a target (`{source's target}`). When combined with a target, the engine natively bridges the grammatical relationship: it converts the owner to "your" or "my" if it's the viewer, naturally injects adjectives (e.g., `{source's glowing target}`), and suppresses the target's article. If your target key contains spaces, you can explicitly demarcate the adjectives from the target key using an additional colon (`{source's glowing:iron sword}`). Plural entities ending in "s" (like "wolves") will correctly render with just an apostrophe ("wolves' target"). 
  * *Unique Proper Nouns:* If a named target is truly unique (like Excalibur), you can prepend the `@` modifier (`{source's @target}`) to force the engine to drop the possessive owner entirely, gracefully outputting "Excalibur" instead of "your Excalibur". If the target is a common noun, the `@` is ignored.
* **Articles / Demonstratives:** You can use `a`, `the`, `this`, `that`, `another`, `one`, `one of the`, and `some` in front of any key to automatically append the article. Indefinite articles ("a") automatically adapt to "some" for plural entities, and demonstratives automatically adapt based on plurality ("this" becomes "these", "that" becomes "those"). Use `{A:key}`, `{The:key}`, etc., to force capitalization mid-sentence. These are automatically suppressed if the entity evaluates to the viewer ("you") or is flagged as a proper noun. You can force an article to render for a proper noun by prepending a plus sign (e.g., `{+this:key}`). You can disable the automatic upgrade of "a" to "the" for previously seen entities by prepending an exclamation mark (e.g., `{!a:key}`).
  * *Best Practice for Plural Proper Nouns:* To represent factions or bands (e.g., "the Avengers", "the Smiths"), include "the" directly in the entity's base name and flag it as a proper noun. The engine will natively suppress any dynamic articles requested by the template, preventing redundant outputs like "the the Avengers".
  * *Ordinals:* If multiple indistinguishable entities are introduced in the same context, the engine automatically upgrades "another" into ordinals ("a third", "a fourth"). For indistinguishable plural entities (e.g., multiple groups of wolves), the engine defaults to "a second set of wolves", but this can be customized by implementing `collective_noun()` on the entity to output natural phrasing like "a second pack of wolves". You can reference them explicitly with definite articles (e.g. `{the:w1}` -> "the first wolf", `{the:w2}` -> "the second wolf"). Ordinals are stable, meaning they persist even if other entities leave the room, and reset automatically when the group drops to a single member. By default, ordinals are rendered as words up to 999, after which they switch to integer form ("1000th"). You can configure this threshold using `ctx.with_ordinal_word_threshold(20)`.
* **Singular Overrides:** Prepend a minus sign (`{-source}`) to force a plural entity to be treated as singular for verbs and pronouns. This is useful when combined with the `one of the` article to target a specific individual in a swarm (e.g., `{One of the:-wolves:Subj} [-wolves:howl]` -> "One of the wolves howls.").
* **Verbs:** `[key:verb]` explicitly binds a base verb to a subject to ensure correct conjugation (including "be" -> "is"/"are" in the present tense, "was"/"were" in the past tense, and "will be" in the future tense). All verbs must be written in their base form (e.g., `[source:attack]`, not `[source:attacked]`) so the engine can shift between tenses. Capitalize the verb (e.g., `[key:Verb]`) to force title-casing mid-sentence. Fully uppercase it (`[key:VERB]`) to output in ALL CAPS. Prepend a plus (`[+key:verb]`) to force 3rd-person conjugation. You can omit the subject key entirely (e.g., `[loom]`); the engine natively parses this and defaults to a 3rd-person singular conjugation, which is convenient for environmental writing (e.g., `"A shadow [loom]."` -> `"A shadow looms."`). You can also bypass conjugation across all perspectives by appending a pipe and the desired form (e.g., `[key:be|be]`). You can provide multiple pipe segments to explicitly define the forms for different perspectives: `[key:freak out|freak out|freaks out]` (base/plural and 3rd-person singular) or `[key:be|am|are|is]` (1st-person singular, 2nd-person/plural, and 3rd-person singular). To provide overrides for the past tense, append a semicolon `;` followed by the past tense forms (e.g., `[key:be|am|are|is;was|were|was]`). You can provide *only* past tense overrides by omitting the present tense segment entirely (e.g., `[key:run|;ran]`). If a tense is omitted, the engine will fall back to native automatic conjugation for that tense. Future tense is generated by prepending "will" to the base verb, and ignores all inline overrides. Phrasal verbs (e.g., `[key:pick up]`) are naturally supported; the engine isolates the first word to ensure `"pick up"` conjugates to `"picks up"`.
  * *Dynamic Verbs:* You can pass a variable into a verb tag using the `$` sigil (e.g., `[source:$action]`). Then, bind the variable to the context using `ctx.with_variable("action", "smile")` or `ctx.with_variables("action", &["smile", "wave"])`. If you pass multiple verbs, the engine will dynamically conjugate all of them and format them as an Oxford comma list (e.g., "smiles, and waves"). This allows you to resolve player emotes dynamically without having to compile a brand new template!
* **Colliding Verbs:** Some verbs share the same base form but have different past tense conjugations depending on their meaning (e.g., "to lie" -> *lay* vs *lied*). To guarantee the intended meaning when shifting to the past tense, annotate the base verb using parentheses: `[source:lie(lay)]` or `[source:lie(lied)]`. If you use an ambiguous base verb without annotation, the template compiler will emit a warning logging the available options, and default to the first dictionary entry.
* **Escaping:** Use a backslash (`\`) to escape special characters if you need to output literal braces or brackets (e.g., `\{`, `\}`, `\[`, `\]`). You can also escape a backslash itself (`\\`), or insert Unicode characters using the `\u{XXXX}` format (e.g., `\u{2764}` for ❤).
* **Line Continuation:** Use a backslash (`\`) at the absolute end of a line to ignore the line break and any leading spaces or tabs on the following line. This is incredibly useful for formatting long, complex templates across multiple lines in your source files without introducing unwanted spaces into the final rendered output.
* **Sentence Boundaries:** To handle edge cases where the Unicode sentence segmenter might fail (e.g., with abbreviations), you can manually control capitalization. Use `[SB]` to force a sentence break (capitalizing the next word, e.g., `wait, [SB]what?` -> "Wait, What?"). Use `[NO_SB]` to prevent a sentence boundary from triggering capitalization (e.g., `vs.[NO_SB] the goblin`).

### **4.1 Continuous and Perfect Tenses**

The engine natively supports all 12 English tenses. Because English forms continuous and perfect tenses analytically (using an auxiliary verb followed by an uninflected participle), you simply target the auxiliary verb (`be` or `have`) with a verb tag and write the participle literally:
* **Continuous:** `{source} [source:be] walking.` (Outputs: *is walking, was walking, will be walking*)
* **Perfect:** `{source} [source:have] walked.` (Outputs: *has walked, had walked, will have walked*)
* **Perfect Continuous:** `{source} [source:have] been walking.` (Outputs: *has been walking, had been walking, will have been walking*)

### **4.2 The Subjunctive Mood**

In English, the subjunctive mood is used to explore hypothetical situations or express wishes and demands. In the subjunctive, verbs **do not inflect**, completely ignoring normal subject-verb agreement (e.g., "I demand that he *leave*" instead of "leaves"). Because these verbs do not change based on the observer, you should simply write the base verb as plain text instead of using a verb tag: `I demand that {target:subj} leave.`
For hypothetical "to be" scenarios, use inline overrides to force the static conjugation: `If I [source:be|were|were|were] you...`

### **4.3 Future Tense and "Do-Support"**

English uses the auxiliary verb "to do" to form negative sentences and questions (e.g., "Aldran *does* not run", "*Does* Aldran run?"). In the future tense, "do" is dropped and replaced with "will" ("Aldran *will* not run", "*Will* Aldran run?"). Because the engine evaluates tags independently and cannot determine if "do" is an auxiliary verb or a main verb (e.g., "Aldran *does* the laundry"), it treats all instances of "do" as main verbs. 

To fix this, annotate the verb as an auxiliary helper: `[source:do(aux)]`. 
*   `{source} [source:do(aux)] not run.`
*   **Present:** "Aldran does not run."
*   **Past:** "Aldran did not run."
*   **Future:** "Aldran will not run." (The engine drops "do" and substitutes "will").

### **5. Smart Pronouns & Anaphora Resolution**

The engine features an Anaphora Resolution system. It allows you to write templates almost entirely with Entity Tags containing pronoun cases (e.g., `{target:Subj} [target:look] at {target:reflex}.`), and dynamically decides when to introduce the full name ("The goblin looks at itself.") and when to use pronouns ("It looks at itself.").

* **How it Triggers:** Whenever the engine encounters a pronoun request on an Entity Tag, it checks the context's memory. If the entity hasn't been introduced yet, it will fall back to using the fully formatted noun and inject the requested article. If a possessive pronoun is requested, the engine parses an `'s` added to the key and appends it during the fallback (e.g., `{the:goblin's:poss}` natively evaluates to `"its"`, but dynamically falls back to `"the goblin's"`).
  * *Standalone Verb Tracking:* Even if an entity is only introduced via a verb tag (e.g., `Bob [bob:attack]`), the engine intercepts the key and injects the entity into the anaphora memory so that subsequent pronouns resolve accurately.
* **Ambiguity Detection:** In plain terms, the engine behaves like a reader. If a pronoun is requested for an entity that isn't the active subject, the engine evaluates the "cast" of recently mentioned characters. If any other recently mentioned character shares the exact same grammatical gender and plurality (e.g., two male characters in the same sentence), the engine recognizes that outputting "He" would confuse the reader. It falls back to the full name to ensure clarity.
* **Auto-Reflexive Upgrades:** If a template uses a standard object pronoun (e.g., `{target:obj}`) and the target happens to be the same entity as the active subject, the engine automatically upgrades the pronoun to its reflexive form (e.g., "himself", "itself", "myself"). This means you don't need to write separate templates for self-inflicted actions!
* **Cross-Context Memory:** The anaphora memory lives inside the `RenderContext`. 
  * **Chaining:** You can render multiple templates in a row using the same context, and the engine will maintain narrative continuity across the templates.
  * **Game Ticks:** If your game loop spans across multiple server ticks or async events, you can extract the full narrative state using `let state = ctx.extract_anaphora()` and inject it into a brand new context later using `RenderContext::new(...).with_anaphora(state)`. This ensures ambiguity detection carries over.
* **Memory Limits (LRU):** To prevent memory from growing unbounded during extremely long, continuous encounters, the anaphora memory acts as a Least-Recently-Used (LRU) cache defaulting to 15 entities. You can configure this limit using `ctx.with_anaphora_limit(size)`.
* **Pinning & Manual Control:** In crowded scenarios, important characters might get pushed out of the LRU cache by a flurry of secondary actors. Protect them by pinning them into memory using `ctx.with_pinned_entity("key")` or `ctx.pin_anaphora("key")`. You can check if an entity is currently pinned using `ctx.is_entity_pinned("key")`. You can unpin them using `ctx.unpin_anaphora("key")`. You can also explicitly remove entities using `ctx.without_anaphora("key")` or `ctx.forget_anaphora("key")`.
* **Forcing Behaviors:** If you explicitly want to suppress the engine's anaphoric article upgrades or pronoun ambiguity fallbacks, you can use the `!` prefix modifier. Writing `{!a:source}` prevents "a" from upgrading to "the". Writing `{source:!subj}` forces the engine to output the pronoun even if it detects an ambiguous collision in the current memory.
* **Querying Rendered State:** The engine tracks exactly how it described an entity to the viewer. You can check if an entity has been introduced to the scene using `ctx.has_seen_entity("key")`. You can retrieve the exact non-pronoun description (including dynamically injected adjectives or ordinals) using `ctx.latest_name("key")`. You can query the most recently assigned integer ordinal using `ctx.current_ordinal("key")` (useful for displaying numeric `[2]` badges in UIs), and retrieve any dynamically injected template adjectives using `ctx.inline_adjectives("key")`. You can also check the current grammatical focus using `ctx.active_subject()`. All of this data is preserved when extracting the `AnaphoraState`.
* **Resetting Memory:** To manually clear the engine's memory, call `ctx.clear_anaphora()`. You should do this whenever narrative continuity is broken to prevent lingering pronoun references. Good times to call this include:
  * When a player moves to a new room or area.
  * When a significant amount of time passes between events.
  * At the start of a new, unrelated combat round or distinct paragraph.
  * *Why?* If you don't clear the memory between independent events, a template might output "He arrives." instead of "Aldran arrives." just because Aldran was the active subject of a completely unrelated event 5 minutes ago!
  * *Auto-Clear:* If your contexts are short-lived or reused sequentially for independent events, you can enable `ctx.with_auto_clear(true)` to have the engine automatically flush the anaphora memory at the end of every successful render call.

#### **Long Display Names & Collision Preemption**

When two entities share the exact same short name (e.g., two entities named "wolf"), the engine automatically checks if either provides a `long_display_name_for` (e.g., "large wolf" or "dire wolf"). If one does, the engine dynamically upgrades the description to disambiguate them before resorting to numbered ordinals (preventing "the first wolf and the second wolf"). 

If you enable the Omniscient Lookahead feature (`ctx.with_lookahead(true)`), the engine will preemptively use the long name on the very first mention. This prevents narrative "pop-in" where an entity is initially introduced as "a wolf" but suddenly upgrades to "the large wolf" sentences later when the second wolf arrives.

#### Adjective Disambiguation

As a final step before falling back to ordinals, the engine will attempt to disambiguate entities by prepending a unique set of adjectives from the `adjectives()` method. For example, if a "large red wolf" and a "large brown wolf" are in the same scene, the engine will recognize that "large" is not a unique descriptor, but "red" and "brown" are. It will automatically render them as "a red wolf" and "a brown wolf" instead of "the first wolf" and "the second wolf".

To prevent exponential evaluation time on entities with many adjectives, the engine restricts its search combinations to the first 5 adjectives returned by the entity. This limit can be adjusted using `ctx.with_adjective_disambiguation_limit(size)`. The absolute maximum allowed size is 63 to prevent mathematical overflow.

#### **Best Practice: Pronoun Tags for Grammatical Case**

Because English nouns do not change form between subjects and objects (e.g., "A goblin attacks" vs "You hit a goblin"), the engine's default noun tags like `{target}` or `{source.minion}` are inherently treated as **nominative (subjective)**. 

If you use a noun tag in the object position of a sentence and that entity evaluates as a First-Person viewer, the engine will output the subjective pronoun "I" instead of "me" (e.g., `"Aldran hits I"`).

**The Solution:** Always use pronoun cases (e.g., `{target:obj}`) to declare the grammatical position of an entity! 
Instead of writing: `{source} [source:hit] {target}.`
You should write: `{source} [source:hit] {target:obj}.`

*Why does this work for NPCs?* If the target is an NPC, you might expect `{target:obj}` to output "Aldran hits him." However, because of the Anaphora Resolution system, if it's the *first* time the NPC has been mentioned, the engine intercepts the pronoun and falls back to the full noun with an indefinite article (e.g., "Aldran hits a goblin."). If it's the viewer, it automatically bypasses the fallback and correctly outputs "Aldran hits me."

#### **Example: Multi-Sentence Combat Log**

Using pronouns and active subject tracking allows builders to write multi-sentence descriptions that adapt to any combination of actors.

```rust
let template = cache.get_or_compile(
    "{A:source:subj} [source:kick] {a:target:obj} in the chest. {The:target:Subj} [target:stumble] backward, and {the:source:subj} [source:press] the advantage!"
)?;

// If Aldran kicks a goblin (Unambiguous pronouns):
// "Aldran kicks a goblin in the chest. It stumbles backward, and he presses the advantage!"

// If the viewer is Aldran (Actor stance takes over):
// "You kick a goblin in the chest. It stumbles backward, and you press the advantage!"

// If Bob (Male) kicks Aldran (Male) -> Ambiguity Resolution prevents "He... he...":
// "Bob kicks Aldran in the chest. Aldran stumbles backward, and Bob presses the advantage!"
```

### **6. Target Resolution**

The `RenderContext` provides a `resolve_target` API to map natural language player commands (like "attack him", "steal Aldran's sword", or "look at the second wolf") back to game entities using the context's active anaphora and ordinal memory.

```rust
use mud_perspective::models::RenderContext;

let ctx = RenderContext::new("char_1")
    .with_entity("goblin1", &goblin1)
    .with_entity("goblin2", &goblin2);

let matches = ctx.resolve_target("the first goblin's sword");
for target_match in &matches {
    println!("Matched key: {}, Path: {:?}", target_match.key, target_match.path);
    if let Some(deep_entity) = target_match.resolve_deep_entity() {
        // Use the resolved nested entity
    }
}
```

The `TargetMatch` struct contains the matched key, the base entity, the requested sub-element path, a `path_uncertain` boolean, and a `resolve_deep_entity()` helper method to fetch the nested item.

* **Pronouns:** Evaluates against `recent_entities` (e.g., "him" matches the last male entity).
* **Ordinals:** Maps words like "second" or postfixes like "wolf 2" to stable ordinals.
* **Deep Target Resolution:** The engine naturally understands possessive relationships from player inputs. It maps phrases like "Aldran's sword" or "his glowing sword" back to their underlying narrative tags (`{source's target}`) or structural data properties (`{source.weapon}`). It also automatically tracks isolated namespaces for ordinals, so a command like "look at Aldran's second sword" resolves directly to the exact sub-entity pointer without requiring you to build your own NLP string parser inside your game's `get_property` implementation!
* **Ambiguity:** Returns multiple matches if an input is vague (e.g., "goblin" might match several).
* **Adjectives & Aliases:** Players can mix and match articles, adjectives, aliases, and names (e.g., "the large angry boss"). The engine ensures the base name or alias matches exactly, while validating any preceding adjectives against the entity's `adjectives()` list.
* **Inline Template Adjectives:** If a template dynamically injects an adjective (e.g., `{source's glowing:sword}`), the engine temporarily stores "glowing" in the scene's memory. This allows players to intuitively type "get glowing sword" immediately after reading it, even if "glowing" isn't normally in the item's database state!
* **Incomplete Possessives:** If a user submits an incomplete possessive (like "take Aldran's"), the engine intentionally returns `0` matches. This prevents the player from targeting the base entity (Aldran himself) and allows your game to respond with "Aldran's what?".

#### TargetMatch & Strict Resolution

The `path_uncertain` field is flagged as `true` if `get_property` fails to find the requested sub-element. Developers can use `ctx.resolve_target_strict("...")` to filter out these invalid paths, ensuring only fully resolvable targets are returned.

#### Entity Aliases

Implement the `aliases()` method on the `TemplateEntity` trait to return a list of alternative names. This allows the engine to strip articles and match alternative names.

```rust
impl TemplateEntity for Character {
    // ... other methods ...

    fn aliases(&self) -> Option<&[&str]> {
        Some(&["boss", "dark lord"])
    }
}
```

#### Adjective Synonyms

Developers can implement the `adjective_synonyms()` method on their `TemplateEntity` to provide a list of alternative adjectives for targeting. This allows players to use more natural language (e.g., "get big sword") to target an item whose canonical adjective is `large`.

These synonyms are only used for targeting and will not be used for rendering, preventing outputs like "the big large sword." 

```rust
impl TemplateEntity for Character {
    // ... other methods ...

    fn adjective_synonyms(&self) -> Option<&[&str]> {
        Some(&["big", "huge"]) // for "large"
    }
}
```

#### Strict Diacritics (Unicode Transliteration)

By default, the target resolution engine transliterates accents and diacritics to their closest ASCII equivalents. This means a player can type "angry wolf" to target an entity named "Ängry Wölf". If a game world relies heavily on constructed languages or specific lore terminology where diacritics matter, builders can disable this fuzzy matching.

```rust
let ctx = RenderContext::new("char_1").with_strict_diacritics(true);
// "angry wolf" will now fail to match "Ängry Wölf" because the player must type it exactly.
```

#### Target Memoization & Caching

The `RenderContext` includes an internal memoization cache for target resolution. If a complex script or combat loop requests `ctx.resolve_target("the goblin")` multiple times in a single server tick, the engine will perform the string parsing and matching once and return *O(1)* cached results for subsequent calls. 

The engine automatically invalidates this cache whenever the narrative state shifts (e.g., adding an entity, changing the stance, or updating the anaphora memory). However, if you mutate an entity's internal properties (like its name or adjectives) using interior mutability while the context is still alive, you should manually call `ctx.clear_target_cache()` to ensure the player's next input resolves against the new data.

### **7. Template Debugging**

The crate includes a command-line utility named `mud_template_tester` to help developers debug templates against grammatical permutations. This tool makes it easy to verify template output across different entity roles, stances, and tenses.

#### Interactive Mode

Running `cargo run --bin mud_template_tester` with no arguments opens an interactive console where you can type templates directly, automatically assign new keys to subsets as needed, and use runtime commands to manage the current test bindings.

In interactive mode, the following commands are available:

* `bind <key>=<subset>` — bind a key to a subset.
* `unbind <key>` — remove a binding.
* `bindings` — list current bindings.
* `exit` / `quit` — leave interactive mode.

#### File Mode

You can also run the utility in file mode with `cargo run --bin mud_template_tester -- [input_file] [output_file]` to batch test templates from a file and write the rendered output to another file. This is useful for regression testing or processing many template variations automatically.

#### Command-Line Options

The `mud_template_tester` binary supports the following command-line flags:

* `--entities <json_file>`: Load custom entities from a JSON file instead of using the default set.
* `--context <json_file>` or `-c`: Evaluate a single specific context rather than iterating through all standard permutations.
* `--bind <key=subset>` or `-b`: Bind a key to a subset at startup.
* `--lookahead` or `-l`: Enable the AST pre-pass for omniscient disambiguation (see Section 2.6 for details).
* `--interactive` or `-i`: Force interactive mode even when input and output file arguments are provided.

#### Custom Entities and Subsets

Evaluating every key against every entity can create exponential noise (for example, "The rusty sword attacks you"). To keep the output manageable, the tester groups entities into combinatorial subsets such as actors and objects.

```json
{
  "subsets": {
    "actors": { "viewer_capable": true },
    "objects": { "viewer_capable": false }
  },
  "entities": [
    { "name": "Aldran", "subset": "actors", "gender": "male" },
    { "name": "rusty sword", "subset": "objects", "is_plural": false }
  ]
}
```

*Providing a simple flat JSON array will gracefully fall back to assigning all entities to the default `actors` subset.*

## **Cargo Features**

By default, `mud_perspective` includes support for parsing and skipping common MUD protocol tags. This prevents the typography engine from capitalizing hidden metadata (e.g., changing `<color red>` to `<color Red>`) and the AST compiler from misinterpreting braces or brackets hidden inside these tags.

* `ansi`: skips ANSI escape sequences (e.g., `\x1b[31m`).
* `mxp`: skips MUD eXtension Protocol HTML-like tags (e.g., `<SEND HREF="look">`).
* `msp`: skips MUD Sound Protocol triggers (e.g., `!!SOUND(roar.wav)`).

These are enabled by default. If your MUD does not use some or all of these protocols, you can disable them to reduce overhead during the compilation and rendering phases:

```toml
[dependencies]
mud_perspective = { version = "0.1", default-features = false, features = ["ansi"] }
```

## **Current Shortcomings**

While functional for standard MUD environments, the current architecture has several limitations that developers should be aware of:

1. **English-Only Morphology:** The verb conjugation and pronoun resolution algorithms are strictly hardcoded for English grammar. Supporting languages with complex declensions or grammatical gender agreement (e.g., Romance or Slavic languages) would require a fundamental rewrite of the grammar.rs module.  
2. **Abbreviation Boundary Detection:** The typography post-processor relies on standard Unicode sentence segmentation to capitalize the first letter of each sentence. Because it lacks a comprehensive natural language abbreviation dictionary, it may incorrectly capitalize words immediately following common abbreviations (e.g., treating the period in "vs. the goblin" as a hard sentence boundary, outputting "vs. The goblin"). Builders must manually annotate these exceptions using the `[NO_SB]` tag.
3. **Past Tense Inflection by Person:** In modern English, "to be" is the only verb that changes form in the past tense based on the subject (*I was, you were*). The engine hardcodes this specific exception, but cannot dynamically handle hypothetical or custom verbs that inflect by person in the past tense.
4. **Multiple Verbs per Tag:** The engine leverages space-splitting to isolate and conjugate the root word of phrasal verbs (e.g., `catch up` -> `catches up`). Consequently, placing entirely separate verbs into a single tag (e.g., `[source:run and jump]`) will fail to conjugate the subsequent verbs. Each verb must be wrapped in its own tag.
5. **Semantic Syntax Limitations:** Because the engine is a rapid morphological formatter rather than an AI-driven NLP parser, it cannot detect contextual sentence structures. This results in the edge cases surrounding the subjunctive mood detailed in Section 4.2.