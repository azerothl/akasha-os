//! Primary rail vs overflow navigation (see docs/UI.md).

use crate::Tab;
use aos_proto::create_contract::MODULE_NAME;

/// Primary rail order: Chat, Agents, Create (optional module), Memory.
pub const PRIMARY_RAIL: [TabKind; 4] = [
    TabKind::Chat,
    TabKind::Agents,
    TabKind::Create,
    TabKind::Memory,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Models overflow destinations as well as the active primary rail.
pub enum TabKind {
    Chat,
    Agents,
    Create,
    Memory,
    Notes,
    Library,
    Files,
    Models,
    Settings,
    Caps,
    Audit,
    Providers,
    Scenarios,
    Feedback,
    Module,
}

#[allow(dead_code)] // Shortcut helpers are retained with the navigation model.
impl TabKind {
    pub fn is_tester(self) -> bool {
        matches!(self, TabKind::Scenarios | TabKind::Feedback)
    }

    /// P1 grouping: Daily (quotidien) vs System vs Admin. Rail tabs
    /// (Chat/Agents/Create/Memory) are excluded — they never render in More.
    pub fn nav_group(self) -> Option<NavGroup> {
        match self {
            TabKind::Notes | TabKind::Library | TabKind::Files | TabKind::Models => {
                Some(NavGroup::Daily)
            }
            TabKind::Providers => Some(NavGroup::System),
            TabKind::Caps
            | TabKind::Audit
            | TabKind::Settings
            | TabKind::Scenarios
            | TabKind::Feedback
            | TabKind::Module => Some(NavGroup::Admin),
            TabKind::Chat | TabKind::Agents | TabKind::Create | TabKind::Memory => None,
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavGroup {
    Daily,
    System,
    Admin,
}

impl NavGroup {
    pub fn label(self, lang: &str) -> &'static str {
        match (self, lang == "fr") {
            (NavGroup::Daily, true) => "Quotidien",
            (NavGroup::Daily, false) => "Daily",
            (NavGroup::System, true) => "Système",
            (NavGroup::System, false) => "System",
            (NavGroup::Admin, true) => "Admin & sûreté",
            (NavGroup::Admin, false) => "Admin & safety",
        }
    }
}

pub fn create_module_tab() -> Tab {
    Tab::Module(MODULE_NAME.into())
}

pub fn is_create_tab(tab: &Tab) -> bool {
    matches!(tab, Tab::Module(name) if name == MODULE_NAME)
}

/// Ordered primary-rail tabs; Create is omitted when the package is not installed.
pub fn primary_rail_tabs(create_installed: bool) -> Vec<Tab> {
    let mut tabs = vec![Tab::Chat, Tab::Agents];
    if create_installed {
        tabs.push(create_module_tab());
    }
    tabs.push(Tab::Memory);
    tabs
}

#[cfg(test)]
pub fn tab_kind(tab: &Tab) -> TabKind {
    match tab {
        Tab::Chat => TabKind::Chat,
        Tab::Agents => TabKind::Agents,
        Tab::Memory => TabKind::Memory,
        Tab::Notes => TabKind::Notes,
        Tab::Library => TabKind::Library,
        Tab::Files => TabKind::Files,
        Tab::Models => TabKind::Models,
        Tab::Settings => TabKind::Settings,
        Tab::Caps => TabKind::Caps,
        Tab::Audit => TabKind::Audit,
        Tab::Providers => TabKind::Providers,
        Tab::Scenarios => TabKind::Scenarios,
        Tab::Feedback => TabKind::Feedback,
        Tab::Module(name) if name == MODULE_NAME => TabKind::Create,
        Tab::Module(_) => TabKind::Module,
    }
}

pub fn is_primary_rail(tab: &Tab) -> bool {
    match tab {
        Tab::Chat | Tab::Agents | Tab::Memory => true,
        Tab::Module(name) => name == MODULE_NAME,
        _ => false,
    }
}

pub fn is_overflow_tab(tab: &Tab) -> bool {
    !is_primary_rail(tab)
}

#[cfg(test)]
pub fn primary_rail_index(tab: &Tab) -> Option<usize> {
    let kind = tab_kind(tab);
    PRIMARY_RAIL.iter().position(|k| *k == kind)
}

pub fn tab_from_primary_index(index: usize, create_installed: bool) -> Option<Tab> {
    primary_rail_tabs(create_installed).get(index).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_rail_has_four_kinds_when_create_installed() {
        assert_eq!(PRIMARY_RAIL.len(), 4);
        assert_eq!(PRIMARY_RAIL[0], TabKind::Chat);
        assert_eq!(PRIMARY_RAIL[2], TabKind::Create);
        assert_eq!(primary_rail_tabs(true).len(), 4);
    }

    #[test]
    fn create_maps_to_module_tab_on_rail() {
        let tab = create_module_tab();
        assert!(is_primary_rail(&tab));
        assert!(is_create_tab(&tab));
        assert_eq!(primary_rail_index(&tab), Some(2));
        assert_eq!(tab_kind(&tab), TabKind::Create);
    }

    #[test]
    fn rail_omits_create_when_uninstalled() {
        let tabs = primary_rail_tabs(false);
        assert_eq!(tabs.len(), 3);
        assert!(!tabs.iter().any(is_create_tab));
        assert_eq!(tab_from_primary_index(2, false), Some(Tab::Memory));
    }

    #[test]
    fn lot4_create_keeps_primary_rail_slot_when_installed() {
        let tabs = primary_rail_tabs(true);
        assert_eq!(tabs.len(), 4);
        assert_eq!(tabs[2], create_module_tab());
        assert_eq!(primary_rail_index(&tabs[2]), Some(2));
    }

    #[test]
    fn scenarios_is_overflow_not_rail() {
        assert!(!is_primary_rail(&Tab::Scenarios));
        assert!(is_overflow_tab(&Tab::Scenarios));
        assert!(TabKind::Scenarios.is_tester());
    }

    #[test]
    fn ctrl_shortcuts_map_to_rail() {
        assert_eq!(tab_from_primary_index(0, true), Some(Tab::Chat));
        assert_eq!(tab_from_primary_index(3, true), Some(Tab::Memory));
        assert_eq!(tab_from_primary_index(4, true), None);
        assert_eq!(TabKind::Agents.keyboard_shortcut(), Some("Ctrl+2"));
        assert_eq!(tab_from_primary_index(2, true), Some(create_module_tab()));
    }

    #[test]
    fn overflow_groups_are_stable() {
        assert_eq!(TabKind::Notes.nav_group(), Some(NavGroup::Daily));
        assert_eq!(TabKind::Files.nav_group(), Some(NavGroup::Daily));
        assert_eq!(TabKind::Models.nav_group(), Some(NavGroup::Daily));
        assert_eq!(TabKind::Providers.nav_group(), Some(NavGroup::System));
        assert_eq!(TabKind::Caps.nav_group(), Some(NavGroup::Admin));
        assert_eq!(TabKind::Chat.nav_group(), None);
        assert_eq!(NavGroup::Daily.label("fr"), "Quotidien");
        assert_eq!(NavGroup::Admin.label("en"), "Admin & safety");
    }
}
