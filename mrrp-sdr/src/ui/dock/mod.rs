pub mod bookmarks;
pub mod channels;
pub mod demodulation;
pub mod radio;
pub mod spectrum;
pub mod waterfall;

use std::collections::HashSet;

use egui::containers::menu::menu_style;
use egui_dock::{
    DockArea,
    Node,
    NodeIndex,
    NodePath,
    Split,
    TabViewer,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::ui::{
    dock::{
        bookmarks::BookmarksDock,
        channels::ChannelsDock,
        demodulation::DemodulationDock,
        radio::RadioDock,
        spectrum::{
            SpectrumDock,
            SpectrumDockState,
        },
        waterfall::WaterfallDock,
    },
    state::{
        AppState,
        CommandBuffer,
    },
};

/// A [`Tab`] uniquely identified by an ID
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Tab {
    id: usize,
    state: TabState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TabState {
    Radio {
        // todo
    },
    Spectrum {
        state: SpectrumDockState,
    },
    Waterfall {
        // todo
    },
    Bookmarks {
        // todo
    },
    Channels {
        // todo
    },
    Demodulation {
        // todo
    },
}

impl TabState {
    pub fn ty(&self) -> TabType {
        match self {
            TabState::Radio { .. } => TabType::Radio,
            TabState::Spectrum { .. } => TabType::Spectrum,
            TabState::Waterfall { .. } => TabType::Waterfall,
            TabState::Bookmarks { .. } => TabType::Bookmarks,
            TabState::Channels { .. } => TabType::Channels,
            TabState::Demodulation { .. } => TabType::Demodulation,
        }
    }

    pub fn title(&self) -> egui::WidgetText {
        // for now we'll just return the generic label for the tab type. but we can also
        // add some extra information from the tab state
        self.ty().label().into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TabType {
    Radio,
    Spectrum,
    Waterfall,
    Bookmarks,
    Channels,
    Demodulation,
}

impl TabType {
    pub fn allow_multiple(&self) -> bool {
        match self {
            _ => true,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Radio => "Radio",
            Self::Spectrum => "Spectrum",
            Self::Waterfall => "Waterfall",
            Self::Bookmarks => "Bookmarks",
            Self::Channels => "Channels",
            Self::Demodulation => "Demodulation",
        }
    }

    pub fn create_state(&self) -> TabState {
        match self {
            TabType::Radio => TabState::Radio {},
            TabType::Spectrum => {
                TabState::Spectrum {
                    state: SpectrumDockState::default(),
                }
            }
            TabType::Waterfall => TabState::Waterfall {},
            TabType::Bookmarks => TabState::Bookmarks {},
            TabType::Channels => TabState::Channels {},
            TabType::Demodulation => TabState::Demodulation {},
        }
    }
}

/// Helper to generate [`Tab`]s from [`TabState`]s
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct TabMaker {
    next_id: usize,
}

impl TabMaker {
    pub fn make_tab(&mut self, state: TabState) -> Tab {
        let id = self.next_id;
        self.next_id += 1;
        Tab { id, state }
    }

    pub fn make_tabs(&mut self, tabs: impl IntoIterator<Item = TabState>) -> Vec<Tab> {
        tabs.into_iter().map(|tab| self.make_tab(tab)).collect()
    }

    pub fn make_default_tabs(&mut self, tabs: impl IntoIterator<Item = TabType>) -> Vec<Tab> {
        tabs.into_iter()
            .map(|tab| self.make_tab(tab.create_state()))
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DockState {
    tab_with_id_maker: TabMaker,
    viewer_state: DockViewerState,
    inner: egui_dock::DockState<Tab>,
}

impl DockState {
    pub fn add_tab(&mut self, path: Option<NodePath>, tab: TabState) {
        let tab = self.tab_with_id_maker.make_tab(tab);

        if let Some(path) = path {
            if let Ok(leaf) = self.inner.leaf_mut(path) {
                leaf.append_tab(tab);
            }
        }
        else {
            self.inner.push_to_focused_leaf(tab);
        }
    }
}

impl Default for DockState {
    fn default() -> Self {
        let mut tab_with_id_maker = TabMaker::default();

        // create dock state with only radio dock
        let mut inner =
            egui_dock::DockState::new(tab_with_id_maker.make_default_tabs([TabType::Radio]));

        let main_surface = inner.main_surface_mut();

        // split radio with spectrum on right
        let [left, right] = main_surface.split(
            NodeIndex::root(),
            Split::Right,
            0.15,
            Node::leaf_with(tab_with_id_maker.make_default_tabs([TabType::Spectrum])),
        );

        // split radio with demodulation on bottom
        main_surface.split(
            left,
            Split::Below,
            0.3,
            Node::leaf_with(tab_with_id_maker.make_default_tabs([TabType::Demodulation])),
        );

        // split spectrum with waterfall on bottom
        main_surface.split(
            right,
            Split::Below,
            0.2,
            Node::leaf_with(tab_with_id_maker.make_default_tabs([TabType::Waterfall])),
        );

        let viewer_state = DockViewerState::new(&inner);

        Self {
            tab_with_id_maker,
            viewer_state,
            inner,
        }
    }
}

#[derive(Debug)]
struct DockViewer<'a> {
    viewer_state: &'a mut DockViewerState,
    command_buffer: &'a mut CommandBuffer,
}

impl<'a> DockViewer<'a> {
    pub fn new(
        viewer_state: &'a mut DockViewerState,
        command_buffer: &'a mut CommandBuffer,
    ) -> Self {
        Self {
            viewer_state,
            command_buffer,
        }
    }
}

impl<'a> TabViewer for DockViewer<'a> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        tab.state.title()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match &mut tab.state {
            TabState::Radio {} => RadioDock.show(ui),
            TabState::Spectrum { state } => SpectrumDock::new(state).show(ui),
            TabState::Waterfall {} => WaterfallDock.show(ui),
            TabState::Bookmarks {} => BookmarksDock.show(ui),
            TabState::Channels {} => ChannelsDock.show(ui),
            TabState::Demodulation {} => DemodulationDock.show(ui),
        }
    }

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new("tab").with(tab.id)
    }

    fn add_popup(&mut self, ui: &mut egui::Ui, path: NodePath) {
        ui.add(DockAddTabMenu {
            path: Some(path),
            command_buffer: &mut self.command_buffer,
            viewer_state: &mut self.viewer_state,
        });
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DockViewerState {
    open_tabs: HashSet<TabType>,
}

impl DockViewerState {
    pub fn new(dock_state: &egui_dock::DockState<Tab>) -> Self {
        let mut open_tabs = HashSet::new();

        for (_, tab) in dock_state.iter_all_tabs() {
            // todo
            let ty = tab.state.ty();

            assert!(ty.allow_multiple() || !open_tabs.contains(&ty));

            open_tabs.insert(ty);
        }

        Self { open_tabs }
    }
}

pub struct DockAddTabMenu<'a> {
    path: Option<NodePath>,
    command_buffer: &'a mut CommandBuffer,
    viewer_state: &'a mut DockViewerState,
}

impl<'a> DockAddTabMenu<'a> {
    pub fn new(app_state: &'a mut AppState, command_buffer: &'a mut CommandBuffer) -> Self {
        Self {
            path: None,
            command_buffer,
            viewer_state: &mut app_state.dock_state.viewer_state,
        }
    }

    fn make_button<'b>(&mut self, ui: &mut egui::Ui, tab: TabType) {
        let enabled = tab.allow_multiple() || !self.viewer_state.open_tabs.contains(&tab);

        if ui
            .add_enabled(enabled, egui::Button::new(tab.label()))
            .clicked()
        {
            let tab = tab.create_state();
            self.command_buffer.add_tab(self.path, tab);
        }
    }
}

impl<'a> egui::Widget for DockAddTabMenu<'a> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.scope(|ui| {
            // apply menu style, so this always looks like a menu
            menu_style(ui.style_mut());

            self.make_button(ui, TabType::Radio);
            self.make_button(ui, TabType::Spectrum);
            self.make_button(ui, TabType::Waterfall);
            self.make_button(ui, TabType::Bookmarks);
            self.make_button(ui, TabType::Channels);
            self.make_button(ui, TabType::Demodulation);
        })
        .response
    }
}

#[derive(Debug)]
pub struct DockPanel<'a> {
    app_state: &'a mut AppState,
    command_buffer: &'a mut CommandBuffer,
}

impl<'a> DockPanel<'a> {
    pub fn new(app_state: &'a mut AppState, command_buffer: &'a mut CommandBuffer) -> Self {
        Self {
            app_state,
            command_buffer,
        }
    }
}

impl<'a> egui::Widget for DockPanel<'a> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        egui::CentralPanel::default()
            .show_inside(ui, |ui| {
                let mut dock_viewer = DockViewer::new(
                    &mut self.app_state.dock_state.viewer_state,
                    &mut self.command_buffer,
                );

                DockArea::new(&mut self.app_state.dock_state.inner)
                    .show_add_buttons(true)
                    .show_add_popup(true)
                    .show_inside(ui, &mut dock_viewer);
            })
            .response
    }
}
