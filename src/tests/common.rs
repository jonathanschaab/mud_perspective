use crate::models::{Gender, TemplateEntity};
use std::borrow::Cow;

/// A mock entity to represent game objects and characters in our tests.
pub struct MockEntity {
    pub id: String,
    pub name: String,
    pub gender: Gender,
    pub is_plural: bool,
    pub is_proper_noun: bool,
}

impl TemplateEntity for MockEntity {
    fn contains_viewer(&self, viewer_id: &str) -> bool {
        self.id == viewer_id
    }

    fn gender(&self) -> Gender {
        self.gender
    }

    fn is_plural(&self) -> bool {
        self.is_plural
    }

    fn display_name_for<'a>(&'a self, viewer_id: &str) -> Cow<'a, str> {
        if self.contains_viewer(viewer_id) {
            return Cow::Borrowed("you");
        }

        // Simulate an epistemological visibility check:
        // If the viewer is a stranger, hide Aldran's real name.
        if viewer_id == "stranger_1" && self.name == "Aldran" {
            Cow::Borrowed("tall man")
        } else if viewer_id == "stranger_1" && self.name == "the Avengers" {
            Cow::Borrowed("masked heroes")
        } else {
            Cow::Borrowed(&self.name)
        }
    }

    fn long_display_name_for<'a>(&'a self, _: &str) -> Option<Cow<'a, str>> {
        if self.id == "mob_2_long" || self.id == "mob_3_long_collide" {
            Some(Cow::Borrowed("large wolf"))
        } else if self.id == "mob_1_scrawny" {
            Some(Cow::Borrowed("scrawny wolf"))
        } else if self.id == "char_jim" {
            Some(Cow::Borrowed("large wolf"))
        } else {
            None
        }
    }

    fn is_proper_noun_for(&self, viewer_id: &str) -> bool {
        // If the stranger sees the masked "tall man", it is no longer a proper noun
        if viewer_id == "stranger_1" && (self.name == "Aldran" || self.name == "the Avengers") {
            false
        } else {
            self.is_proper_noun
        }
    }
}

pub struct ConfigurableMockEntity {
    pub id: String,
    pub name: String,
    pub long_name: Option<String>,
    pub gender: Gender,
}

impl TemplateEntity for ConfigurableMockEntity {
    fn contains_viewer(&self, viewer_id: &str) -> bool {
        self.id == viewer_id
    }
    fn gender(&self) -> Gender {
        self.gender
    }
    fn is_plural(&self) -> bool {
        self.gender == Gender::Plural
    }
    fn is_proper_noun_for(&self, _: &str) -> bool {
        false
    }
    fn display_name_for<'a>(&'a self, _: &str) -> Cow<'a, str> {
        Cow::Borrowed(&self.name)
    }
    fn long_display_name_for<'a>(&'a self, _: &str) -> Option<Cow<'a, str>> {
        self.long_name.as_deref().map(Cow::Borrowed)
    }
}
