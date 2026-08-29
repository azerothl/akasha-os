//! Primary rail vs overflow navigation (see docs/UI.md).

use crate::Tab;

/// Primary rail order: Chat, Agents, Create (Image), Memory.
pub const PRIMARY_RAIL: [TabKind; 4] = [
    TabKind::Chat,
    TabKind::Agents,
    TabKind::Create,
    TabKind::Memory,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    Chat,
    Agents,
    Create,
    Memory,
    Notes,
    Library,
    Tasks,
    Models,
    Settings,
    Caps,
    Audit,
    Providers,
    Scenarios,
    Feedback,
    Module,
}

impl TabKind {
    pub fn is_tester(self) -> bool {
        matches!(self, TabKind::Scenarios | TabKind::Feedback)
    }

    pub fn keyboard_shortcut(self) -> Option<&'static str> {
        PRIMARY_RAIL
            .iter()
            .position(|k| *k == self)
            .map(|i| match i {
                0 => "Ctrl+1",
                1 => "Ctrl+2",
                2 => "Ctrl+3",
                3 => "Ctrl+4",
                _ => unreachable!(),
            })
    }
}

pub fn tab_kind(tab: &Tab) -> TabKind {
    match tab {
        Tab::Chat => TabKind::Chat,
        Tab::Agents => TabKind::Agents,
        Tab::Image => TabKind::Create,
        Tab::Memory => TabKind::Memory,
        Tab::Notes => TabKind::Notes,
        Tab::Library => TabKind::Library,
        Tab::Tasks => TabKind::Tasks,
        Tab::Models => TabKind::Models,
        Tab::Settings => TabKind::Settings,
        Tab::Caps => TabKind::Caps,
        Tab::Audit => TabKind::Audit,
        Tab::Providers => TabKind::Providers,
        Tab::Scenarios => TabKind::Scenarios,
        Tab::Feedback => TabKind::Feedback,
        Tab::Module(_) => TabKind::Module,
    }
}

pub fn is_primary_rail(tab: &Tab) -> bool {
    matches!(
        tab,
        Tab::Chat | Tab::Agents | Tab::Image | Tab::Memory
    )
}

pub fn is_overflow_tab(tab: &Tab) -> bool {
    !is_primary_rail(tab)
}

pub fn primary_rail_index(tab: &Tab) -> Option<usize> {
    let kind = tab_kind(tab);
    PRIMARY_RAIL.iter().position(|k| *k == kind)
}

pub fn tab_from_primary_index(index: usize) -> Option<Tab> {
    let kind = PRIMARY_RAIL.get(index)?;
    Some(match kind {
        TabKind::Chat => Tab::Chat,
        TabKind::Agents => Tab::Agents,
        TabKind::Create => Tab::Image,
        TabKind::Memory => Tab::Memory,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_rail_has_four_items() {
        assert_eq!(PRIMARY_RAIL.len(), 4);
        assert_eq!(PRIMARY_RAIL[0], TabKind::Chat);
        assert_eq!(PRIMARY_RAIL[2], TabKind::Create);
    }

    #[test]
    fn image_maps_to_create_on_rail() {
        assert!(is_primary_rail(&Tab::Image));
        assert_eq!(primary_rail_index(&Tab::Image), Some(2));
    }

    #[test]
    fn scenarios_is_overflow_not_rail() {
        assert!(!is_primary_rail(&Tab::Scenarios));
        assert!(is_overflow_tab(&Tab::Scenarios));
        assert!(TabKind::Scenarios.is_tester());
    }

    #[test]
    fn ctrl_shortcuts_map_to_rail() {
        assert_eq!(tab_from_primary_index(0), Some(Tab::Chat));
        assert_eq!(tab_from_primary_index(3), Some(Tab::Memory));
        assert_eq!(tab_from_primary_index(4), None);
        assert_eq!(TabKind::Agents.keyboard_shortcut(), Some("Ctrl+2"));
    }
}
