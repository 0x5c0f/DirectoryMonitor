/// Query parameters for event storage.
#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    pub limit: usize,
    pub offset: usize,
    pub event_types: Vec<String>,
    pub watch_root: Option<String>,
    pub search: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub is_dir: Option<bool>,
}

impl EventQuery {
    /// Create a simple page query.
    pub fn page(limit: usize, offset: usize) -> Self {
        Self {
            limit,
            offset,
            ..Self::default()
        }
    }
}
