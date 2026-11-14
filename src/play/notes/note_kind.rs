use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct NoteKind {
    pub id: String,
    pub description: String,
    pub note_style_id: Option<String>,
    pub no_anim: bool,
    pub suffix: String,
    pub scoreable: bool,
    pub low_priority: bool,
}

impl Default for NoteKind {
    fn default() -> Self {
        Self {
            id: String::new(),
            description: String::new(),
            note_style_id: None,
            no_anim: false,
            suffix: String::new(),
            scoreable: true,
            low_priority: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoteKindManager {
    kinds: HashMap<String, NoteKind>,
}

impl Default for NoteKindManager {
    fn default() -> Self {
        let mut mgr = Self {
            kinds: HashMap::new(),
        };
        mgr.register_builtins();
        mgr
    }
}

impl NoteKindManager {
    pub fn register_builtins(&mut self) {
        self.register(NoteKind {
            id: "noanim".to_string(),
            description: "No Animation".to_string(),
            no_anim: true,
            ..Default::default()
        });
        self.register(NoteKind {
            id: "non_scoreable".to_string(),
            description: "Non-scoreable".to_string(),
            scoreable: false,
            ..Default::default()
        });
        self.register(NoteKind {
            id: "alt".to_string(),
            description: "Alt Animation".to_string(),
            suffix: "-alt".to_string(),
            ..Default::default()
        });
        self.register(NoteKind {
            id: "mom".to_string(),
            description: "Mom Sing".to_string(),
            ..Default::default()
        });
    }

    pub fn register(&mut self, kind: NoteKind) {
        self.kinds.insert(kind.id.clone(), kind);
    }

    pub fn get(&self, kind_id: Option<&str>) -> Option<&NoteKind> {
        let id = kind_id?;
        if id.is_empty() {
            return None;
        }
        self.kinds.get(id)
    }

    pub fn is_scoreable(&self, kind_id: Option<&str>) -> bool {
        self.get(kind_id).map(|k| k.scoreable).unwrap_or(true)
    }

    pub fn is_no_anim(&self, kind_id: Option<&str>) -> bool {
        self.get(kind_id).map(|k| k.no_anim).unwrap_or(false)
    }

    pub fn anim_suffix(&self, kind_id: Option<&str>) -> &str {
        self.get(kind_id).map(|k| k.suffix.as_str()).unwrap_or("")
    }

    pub fn resolve_style_id(&self, kind_id: Option<&str>) -> Option<&str> {
        self.get(kind_id).and_then(|k| k.note_style_id.as_deref())
    }
}
