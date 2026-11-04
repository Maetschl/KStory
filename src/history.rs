use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryLimit {
    Bounded(usize),
    Unlimited,
}

impl Default for HistoryLimit {
    fn default() -> Self {
        Self::Bounded(100)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryEntry<T> {
    pub(crate) label: String,
    pub(crate) before: T,
    pub(crate) after: T,
    coalesce_key: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct History<T> {
    undo: VecDeque<HistoryEntry<T>>,
    redo: Vec<HistoryEntry<T>>,
    saved: T,
    limit: HistoryLimit,
}

impl<T: Clone + PartialEq> History<T> {
    pub(crate) fn new(initial: T, limit: HistoryLimit) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            saved: initial,
            limit,
        }
    }

    pub(crate) fn record(
        &mut self,
        label: impl Into<String>,
        before: T,
        after: T,
        coalesce_key: Option<String>,
    ) {
        if before == after {
            return;
        }

        let label = label.into();
        if let Some(key) = &coalesce_key {
            if let Some(last) = self.undo.back_mut() {
                if last.coalesce_key.as_ref() == Some(key) && self.redo.is_empty() {
                    last.after = after;
                    last.label = label;
                    if last.before == last.after {
                        self.undo.pop_back();
                    }
                    return;
                }
            }
        }

        self.redo.clear();
        self.undo.push_back(HistoryEntry {
            label,
            before,
            after,
            coalesce_key,
        });
        self.trim_to_limit();
    }

    pub(crate) fn undo(&mut self) -> Option<(String, T)> {
        let entry = self.undo.pop_back()?;
        let label = entry.label.clone();
        let state = entry.before.clone();
        self.redo.push(entry);
        Some((label, state))
    }

    pub(crate) fn redo(&mut self) -> Option<(String, T)> {
        let entry = self.redo.pop()?;
        let label = entry.label.clone();
        let state = entry.after.clone();
        self.undo.push_back(entry);
        self.trim_to_limit();
        Some((label, state))
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|entry| entry.label.as_str())
    }

    pub(crate) fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|entry| entry.label.as_str())
    }

    pub(crate) fn is_dirty(&self, current: &T) -> bool {
        &self.saved != current
    }

    pub(crate) fn mark_saved(&mut self, current: &T) {
        self.saved = current.clone();
    }

    pub(crate) fn reset(&mut self, current: T) {
        self.undo.clear();
        self.redo.clear();
        self.saved = current;
    }

    pub(crate) fn set_limit(&mut self, limit: HistoryLimit) {
        self.limit = limit;
        self.trim_to_limit();
    }

    fn trim_to_limit(&mut self) {
        if let HistoryLimit::Bounded(limit) = self.limit {
            while self.undo.len() > limit {
                self.undo.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_and_branching_work() {
        let mut history = History::new(0, HistoryLimit::default());
        history.record("one", 0, 1, None);
        history.record("two", 1, 2, None);
        assert_eq!(history.undo().map(|(_, state)| state), Some(1));
        history.record("branch", 1, 3, None);
        assert!(!history.can_redo());
        assert_eq!(history.undo().map(|(_, state)| state), Some(1));
        assert_eq!(history.redo().map(|(_, state)| state), Some(3));
    }

    #[test]
    fn no_op_and_limits_are_handled() {
        let mut history = History::new(0, HistoryLimit::Bounded(2));
        history.record("noop", 0, 0, None);
        history.record("one", 0, 1, None);
        history.record("two", 1, 2, None);
        history.record("three", 2, 3, None);
        assert_eq!(history.undo().map(|(_, state)| state), Some(2));
        assert_eq!(history.undo().map(|(_, state)| state), Some(1));
        assert!(!history.can_undo());
    }

    #[test]
    fn consecutive_text_edits_coalesce() {
        let mut history = History::new(String::new(), HistoryLimit::default());
        history.record("text", String::new(), "a".into(), Some("field".into()));
        history.record("text", "a".into(), "ab".into(), Some("field".into()));
        assert_eq!(history.undo().map(|(_, state)| state), Some(String::new()));
    }

    #[test]
    fn clean_state_and_unlimited_history_are_supported() {
        let mut history = History::new(0, HistoryLimit::Bounded(1));
        history.record("one", 0, 1, None);
        history.mark_saved(&1);
        assert!(!history.is_dirty(&1));
        history.record("two", 1, 2, None);
        history.set_limit(HistoryLimit::Unlimited);
        history.record("three", 2, 3, None);
        assert!(history.can_undo());
        assert_eq!(history.undo().map(|(_, state)| state), Some(2));
        assert_eq!(history.undo().map(|(_, state)| state), Some(1));
    }
}
