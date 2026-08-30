//! Tab lifecycle management for the embedded browser: user tabs and agent
//! tabs, attach/detach/close/select, with origin isolation.

use std::collections::HashMap;

use uuid::Uuid;

/// Who owns a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabOrigin {
    User,
    Agent,
}

/// A live browser tab.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: Uuid,
    pub url: String,
    pub title: String,
    pub origin: TabOrigin,
    /// The active session that claimed the tab (agent tabs).
    pub owner_session: Option<Uuid>,
}

/// Manages the tab map.
#[derive(Debug, Default)]
pub struct TabManager {
    tabs: HashMap<Uuid, Tab>,
    active: Option<Uuid>,
}

impl TabManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a tab and returns its id.
    pub fn create(&mut self, url: &str, origin: TabOrigin, owner_session: Option<Uuid>) -> Uuid {
        let id = Uuid::new_v4();
        self.tabs.insert(
            id,
            Tab {
                id,
                url: url.to_string(),
                title: String::new(),
                origin,
                owner_session,
            },
        );
        self.active = Some(id);
        id
    }

    pub fn get(&self, id: Uuid) -> Option<&Tab> {
        self.tabs.get(&id)
    }

    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Tab> {
        self.tabs.get_mut(&id)
    }

    pub fn close(&mut self, id: Uuid) {
        self.tabs.remove(&id);
        if self.active == Some(id) {
            self.active = self.tabs.keys().next().copied();
        }
    }

    pub fn select(&mut self, id: Uuid) -> bool {
        if self.tabs.contains_key(&id) {
            self.active = Some(id);
            true
        } else {
            false
        }
    }

    pub fn active(&self) -> Option<Uuid> {
        self.active
    }

    pub fn tabs(&self) -> Vec<Tab> {
        let mut out: Vec<Tab> = self.tabs.values().cloned().collect();
        out.sort_by_key(|t| t.id.to_string());
        out
    }

    /// User-owned tabs (the UI shows these; agent tabs are transient).
    pub fn user_tabs(&self) -> Vec<Tab> {
        self.tabs.values().filter(|t| t.origin == TabOrigin::User).cloned().collect()
    }

    /// Releases an agent tab back to the user.
    pub fn release_to_user(&mut self, id: Uuid) -> bool {
        if let Some(tab) = self.tabs.get_mut(&id) {
            if tab.origin == TabOrigin::Agent {
                tab.origin = TabOrigin::User;
                tab.owner_session = None;
                return true;
            }
        }
        false
    }

    /// Claims a user tab for an agent session.
    pub fn claim_tab(&mut self, id: Uuid, session: Uuid) -> bool {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.origin = TabOrigin::Agent;
            tab.owner_session = Some(session);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_select_close_cycle() {
        let mut tm = TabManager::new();
        let a = tm.create("https://example.com", TabOrigin::User, None);
        let b = tm.create("https://other.com", TabOrigin::User, None);
        assert_eq!(tm.active(), Some(b)); // last created is active
        assert!(tm.select(a));
        assert_eq!(tm.active(), Some(a));
        tm.close(a);
        assert_eq!(tm.tabs().len(), 1);
    }

    #[test]
    fn agent_tabs_are_isolated() {
        let mut tm = TabManager::new();
        let user = tm.create("https://a.com", TabOrigin::User, None);
        let agent = tm.create("https://b.com", TabOrigin::Agent, Some(Uuid::new_v4()));
        assert_eq!(tm.user_tabs().len(), 1);
        assert_eq!(tm.user_tabs()[0].id, user);

        // release the agent tab to the user
        assert!(tm.release_to_user(agent));
        assert_eq!(tm.user_tabs().len(), 2);
        assert_eq!(tm.tabs().iter().find(|t| t.id == agent).unwrap().origin, TabOrigin::User);
    }

    #[test]
    fn claim_user_tab_for_agent() {
        let mut tm = TabManager::new();
        let user = tm.create("https://a.com", TabOrigin::User, None);
        let session = Uuid::new_v4();
        assert!(tm.claim_tab(user, session));
        let tab = tm.get(user).unwrap();
        assert_eq!(tab.origin, TabOrigin::Agent);
        assert_eq!(tab.owner_session, Some(session));
    }
}
