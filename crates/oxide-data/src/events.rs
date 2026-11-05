use std::collections::BTreeSet;

use crate::chart::SongEvent;

/// The set of song event discriminators known to the engine.
#[derive(Debug, Clone, Default)]
pub struct EventRegistry {
    names: BTreeSet<String>,
}

impl EventRegistry {
    /// Base events shipped by V-Slice, plus additional discriminators verified
    /// in `funkin/play/event/*SongEvent.hx`.
    pub fn base() -> Self {
        let mut registry = Self::default();
        for name in [
            "EnableMask",
            "FocusCamera",
            "PlayAnimation",
            "ScrollSpeed",
            "SetCameraBop",
            "SetHealthIcon",
            "SetTargetBopSpeed",
            "ZoomCamera",
        ] {
            registry.register(name);
        }
        registry
    }

    /// Returns false if already present.
    pub fn register(&mut self, name: impl Into<String>) -> bool {
        self.names.insert(name.into())
    }

    pub fn knows(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.names.iter().cloned().collect()
    }

    pub fn unknown_event_names(&self, events: &[SongEvent]) -> Vec<String> {
        events
            .iter()
            .filter(|event| !self.knows(&event.event))
            .map(|event| event.event.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}
