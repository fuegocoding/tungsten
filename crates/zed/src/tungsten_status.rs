//! Tungsten vault status bar item.
//!
//! Shows the current vault's name in the status bar when a vault
//! is open. Renders nothing (a hidden div) when no vault is open
//! or when the user has hidden the item via Settings → Status Bar.
//!
//! The vault name is sourced from a [`gpui::Global`] singleton
//! (`TungstenVaultName`) that `handle_open_request` updates on
//! every CLI open. The status bar item reads from the same global
//! so the two stay in sync.

use gpui::{Context, Global, Render, SharedString, Window};

use workspace::StatusItemView;
use ui::{prelude::*, ButtonLike, Icon, IconName, Tooltip};

/// Global storage for the active vault's display name. Set by
/// `handle_open_request` when a CLI open resolves to a vault;
/// read by [`TungstenVaultStatusItem`] in the status bar.
#[derive(Debug, Default, Clone)]
pub struct TungstenVaultName(pub Option<String>);

impl Global for TungstenVaultName {}

/// The status bar item. Always created; renders nothing when no
/// vault is active.
pub struct TungstenVaultStatusItem {
    workspace: gpui::WeakEntity<workspace::Workspace>,
}

impl TungstenVaultStatusItem {
    pub fn new(workspace: gpui::WeakEntity<workspace::Workspace>) -> Self {
        Self { workspace }
    }
}

impl Render for TungstenVaultStatusItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let vault_name = cx
            .try_global::<TungstenVaultName>()
            .and_then(|g| g.0.clone());
        let Some(name) = vault_name else {
            return div().hidden();
        };
        div().child(
            ButtonLike::new("tungsten-vault")
                .child(
                    h_flex()
                        .gap_1()
                        .child(Icon::new(IconName::Book))
                        .child(SharedString::from(name)),
                )
                .tooltip(Tooltip::text(
                    "Tungsten vault — this folder contains a .obsidian/ directory",
                )),
        )
    }
}

impl StatusItemView for TungstenVaultStatusItem {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn workspace::ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // No state tied to the active pane.
    }

    fn hide_setting(
        &self,
        _: &gpui::App,
    ) -> Option<workspace::HideStatusItem> {
        // The HideStatusItem type lives in workspace's status_bar
        // module but isn't re-exported publicly; without
        // extending the StatusBarSettings schema we can't
        // expose a per-item hide toggle. Users can hide the
        // entire status bar via Settings → Status Bar → Show.
        None
    }
}
