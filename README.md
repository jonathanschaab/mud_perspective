# **mud\_perspective**

mud\_perspective is a Rust library designed to handle perspective-aware text generation for Multi-User Dungeons (MUDs) and interactive fiction. In multiplayer text environments, a single event must often be described differently depending on who is observing it. This engine resolves pronouns, conjugations, and indefinite articles dynamically based on the observer's relationship to the event.

## **Goals**

The primary goal of this crate is to provide a reliable, thread-safe templating system for game servers. Its core objectives include:

* **Stance Shifting:** Automatically transitioning between "Actor Stance" (addressing the viewer in the first/second person) and "Director Stance" (addressing the viewer in the third person).

* **Subject-Verb Agreement:** Conjugating verbs correctly based on the grammatical number and person of the subject.  
* **Irregular Verb Handling:** Utilizing a static dictionary to safely conjugate common irregular and modal verbs without relying strictly on algorithmic suffixes.  
* **Epistemological Masking:** Allowing the underlying game logic to obscure entity names (e.g., using disguises or recognition systems) based on the specific observer viewing the text.
* **Memory Efficiency:** Using a thread-safe AST cache backed by an LRU eviction strategy and Cow (Clone-on-Write) strings to minimize heap allocations during high-frequency combat loops.

## **API Usage**

### **1\. Implementing TemplateEntity**

To use the engine, your game objects must implement the TemplateEntity trait. This abstracts your database logic away from the rendering engine.rust

use mud\_perspective::models::{TemplateEntity, Gender};

use std::borrow::Cow;

pub struct Character {

pub id: String,

pub name: String,

pub gender: Gender,

pub is\_plural: bool,

pub is\_proper\_noun: bool,

}

impl TemplateEntity for Character {

fn contains\_viewer(\&self, viewer\_id: \&str) \-\> bool {

self.id \== viewer\_id

}

fn gender(\&self) \-\> Gender { self.gender }

fn is\_plural(\&self) \-\> bool { self.is\_plural }

fn is\_proper\_noun\_for(\&self, \_viewer\_id: \&str) \-\> bool {   
    self.is\_proper\_noun   
}

fn display\_name\_for\<'a\>(&'a self, \_viewer\_id: \&str) \-\> Cow\<'a, str\> {  
    // You can implement disguise logic or recognition checks here  
    Cow::Borrowed(\&self.name)  
}

}

\#\#\# 2\. Rendering Templates  
The engine provides a \`render\_msg\!\` macro to make evaluating templates against a context ergonomic for game logic.

\`\`\`rust  
use mud\_perspective::{render\_msg, TemplateCache};

// Initialize this cache once and share it across your game state  
let cache \= TemplateCache::new(1000);

// Compile the template  
let template \= cache.get\_or\_compile("{the:source} \[source:watch\] as {the:target} \[target:approach\].").unwrap();

let player \= Character { /\*... \*/ };  
let goblin \= Character { /\*... \*/ };

// Actor Stance (The player is the viewer)  
let output\_actor \= render\_msg\!("char\_1", \&template, "source" \=\> \&player, "target" \=\> \&goblin).unwrap();  
// Output: "You watch as the goblin approaches."

// Director Stance (A third-party bystander is the viewer)  
let output\_director \= render\_msg\!("char\_3", \&template, "source" \=\> \&player, "target" \=\> \&goblin).unwrap();  
// Output: "Aldran watches as the goblin approaches."

### **3\. Syntax Reference**

* **Entities:** {key} inserts the entity's display name.  
* **Articles:** {a:key} or {the:key} prepends the appropriate article. These are automatically suppressed if the entity evaluates to the viewer ("you") or is flagged as a proper noun.  
* **Pronouns:** {key:type}. Supported types include subj (he/she/it/they), obj (him/her/it/them), poss (his/her/their), abs\_poss (his/hers/theirs), and reflex (himself/themselves).

* **Verbs:** \[key:verb\] explicitly binds a base verb to a subject to ensure correct conjugation. This prevents grammatical errors during compound subjects or passive voice structures.

## **Current Shortcomings**

While functional for standard MUD environments, the current architecture has several limitations that developers should be aware of:

1. **English-Only Morphology:** The verb conjugation and pronoun resolution algorithms are strictly hardcoded for English grammar. Supporting languages with complex declensions or grammatical gender agreement (e.g., Romance or Slavic languages) would require a fundamental rewrite of the grammar.rs module.  
2. **Abbreviation Boundary Detection:** The typography post-processor relies on standard Unicode sentence segmentation to capitalize the first letter of each sentence. Because it lacks a comprehensive natural language abbreviation dictionary, it may incorrectly capitalize words immediately following common abbreviations (e.g., treating the period in "Mr. Smith" as a hard sentence boundary).

3. **Static Irregular Verb Map:** The internal Perfect Hash Function (PHF) map currently only covers a curated core set of irregular and modal verbs. Verbs outside of this list fall back to algorithmic suffix rules (adding "s", "es", or "ies"), which will produce grammatically incorrect text for unmapped irregulars.  
4. **No Anaphora Resolution:** The engine evaluates syntax strictly token-by-token. It cannot contextually look back at previous sentences to determine if a noun has already been introduced, meaning it cannot automatically decide when to switch from an indefinite article ("a sword") to a definite article ("the sword") across larger paragraphs of text.