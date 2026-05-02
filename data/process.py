import json
import os

def is_consonant(char):
    return char.lower() in "bcdfghjklmnpqrstvwxyz"

def get_regular_conjugations(base):
    """Generates strictly regular conjugations for a given base verb."""
    
    # 1. 3rd Person Singular
    if len(base) > 1 and base[-1] == 'y' and is_consonant(base[-2]):
        s_form = base[:-1] + "ies"
    elif base.endswith(('s', 'x', 'z', 'ch', 'sh', 'o')):
        s_form = base + "es"
    else:
        s_form = base + "s"

    # 2. Past Simple / Past Participle
    if base.endswith('e'):
        ed_form = base + "d"
    elif len(base) > 1 and base[-1] == 'y' and is_consonant(base[-2]):
        ed_form = base[:-1] + "ied"
    else:
        ed_form = base + "ed"

    # 3. Present Participle
    if base.endswith('ie'):
        ing_form = base[:-2] + "ying"
    elif base.endswith('e') and not base.endswith(('ee', 'oe', 'ye')):
        ing_form = base[:-1] + "ing"
    else:
        ing_form = base + "ing"

    return s_form, ed_form, ed_form, ing_form

def process_verbs(input_file):
    # Load the original JSON
    with open(input_file, 'r', encoding='utf-8') as f:
        all_verbs = json.load(f)

    # Group by base verb to detect collisions, tracking the original index
    verb_groups = {}
    for index, entry in enumerate(all_verbs):
        if not entry: 
            continue # Skip entirely empty entries
            
        base = entry[0]
        if base not in verb_groups:
            verb_groups[base] = []
        # Store a tuple of (original_index, entry_data)
        verb_groups[base].append((index, entry))

    irregular_verbs = []
    colliding_verbs = []
    malformed_entries = 0

    for base, entries in verb_groups.items():
        # Handle Collisions (Multiple entries for the same base verb)
        if len(entries) > 1:
            for original_index, entry in entries:
                colliding_verbs.append(entry)
            continue

        # Handle Single Entries
        original_index, entry = entries[0]
        
        # Try to unpack, catch the error if the list doesn't have exactly 5 items
        try:
            base, actual_s, actual_past, actual_past_part, actual_ing = entry
        except ValueError:
            print(f"MALFORMED ENTRY at JSON Index {original_index}: {entry}")
            malformed_entries += 1
            continue # Skip processing this entry and move to the next one
        
        # Generate what the strict rules *think* it should be
        expected_s, expected_past, expected_past_part, expected_ing = get_regular_conjugations(base)

        # Compare actual to expected
        is_regular = (
            actual_s == expected_s and
            actual_past == expected_past and
            actual_past_part == expected_past_part and
            actual_ing == expected_ing
        )

        if not is_regular:
            irregular_verbs.append(entry)

    # Output Irregular Verbs
    with open('irregular_verbs.json', 'w', encoding='utf-8') as f:
        json.dump(irregular_verbs, f, indent=4)
    
    # Output Colliding Verbs
    with open('colliding_verbs.json', 'w', encoding='utf-8') as f:
        json.dump(colliding_verbs, f, indent=4)

    print("\n--- Processing Complete ---")
    print(f"Processed {len(all_verbs)} total entries.")
    print(f"Skipped {malformed_entries} malformed entries.")
    print(f"Found {len(irregular_verbs)} irregular verbs.")
    print(f"Found {len(colliding_verbs)} colliding entries.")

if __name__ == "__main__":
    process_verbs('verbs-dictionaries.json')