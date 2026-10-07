//! GPUI-free receipts and keyboard selection for the native AE import dialog.
//! Only an explicitly authorized Save-and-continue may advance a file receipt.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    operation: u64,
    document: u64,
    context: u64,
}
impl Receipt {
    pub fn capture(operation: u64, document: u64, context: u64) -> Self {
        Self {
            operation,
            document,
            context,
        }
    }
    pub fn current(self, operation: u64, document: u64, context: u64) -> bool {
        self.operation == operation && self.document == document && self.context == context
    }
    pub fn advance_save(&mut self, previous: u64, next: u64) -> bool {
        if self.operation != previous {
            return false;
        }
        self.operation = next;
        true
    }
}

pub fn list_selection(selected: Option<usize>, count: usize, key: &str) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let selected = selected.filter(|index| *index < count);
    match key {
        "home" => Some(0),
        "end" => Some(count - 1),
        "up" | "left" => Some(selected.unwrap_or(0).saturating_sub(1)),
        "down" | "right" => Some(selected.map_or(0, |index| (index + 1).min(count - 1))),
        _ => selected,
    }
}

/// List, Cancel, Apply. Disabled Apply never receives keyboard focus.
pub fn next_focus(current: usize, backwards: bool, can_apply: bool) -> usize {
    let count = if can_apply { 3 } else { 2 };
    let current = current.min(count - 1);
    if backwards {
        (current + count - 1) % count
    } else {
        (current + 1) % count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_receipts_reject_supersession_and_document_aba() {
        let receipt = Receipt::capture(4, 8, 10);
        assert!(receipt.current(4, 8, 10));
        assert!(!receipt.current(5, 8, 10));
        assert!(!receipt.current(4, 9, 10));
        assert!(!receipt.current(4, 8, 11));
    }
    #[test]
    fn only_the_exact_save_chain_can_advance_a_receipt() {
        let mut receipt = Receipt::capture(4, 8, 10);
        assert!(!receipt.advance_save(5, 6));
        assert!(receipt.current(4, 8, 10));
        assert!(receipt.advance_save(4, 5));
        assert!(!receipt.current(4, 8, 10));
        assert!(receipt.current(5, 8, 10));
        assert!(!receipt.current(5, 8, 11));
        assert!(receipt.advance_save(5, 6));
    }
    #[test]
    fn selection_is_bounded_and_empty_lists_are_inert() {
        for key in ["up", "down", "left", "right", "home", "end"] {
            assert_eq!(list_selection(Some(20), 0, key), None);
        }
        assert_eq!(list_selection(None, 4, "down"), Some(0));
        assert_eq!(list_selection(Some(0), 4, "up"), Some(0));
        assert_eq!(list_selection(Some(3), 4, "down"), Some(3));
        assert_eq!(list_selection(Some(1), 4, "home"), Some(0));
        assert_eq!(list_selection(Some(1), 4, "end"), Some(3));
        assert_eq!(list_selection(Some(100), 4, "right"), Some(0));
    }
    #[test]
    fn tab_wraps_without_visiting_disabled_apply() {
        assert_eq!(next_focus(0, true, false), 1);
        assert_eq!(next_focus(1, false, false), 0);
        assert_eq!(next_focus(2, false, false), 0);
        assert_eq!(next_focus(0, true, true), 2);
        assert_eq!(next_focus(2, false, true), 0);
    }
}
