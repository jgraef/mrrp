pub mod bookmarks;
pub mod channels;
pub mod demodulation;
pub mod spectrum;
pub mod waterfall;

use egui::containers::menu::menu_style;
use egui_dock::{
    DockState,
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
        spectrum::SpectrumDock,
        waterfall::WaterfallDock,
    },
    state::CommandBuffer,
};

#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub enum Tab {
    Spectrum,
    Waterfall,
    Bookmarks,
    Channels,
    Demodulation,
}

#[derive(Debug)]
pub struct DockViewer<'a> {
    command_buffer: &'a mut CommandBuffer,
}

impl<'a> DockViewer<'a> {
    pub fn new(command_buffer: &'a mut CommandBuffer) -> Self {
        Self { command_buffer }
    }
}

impl<'a> TabViewer for DockViewer<'a> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let title = match tab {
            Tab::Spectrum => "Spectrum",
            Tab::Waterfall => "Waterfall",
            Tab::Bookmarks => "Bookmarks",
            Tab::Channels => "Channels",
            Tab::Demodulation => "Demodulation",
        };

        title.into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Spectrum => SpectrumDock.show(ui),
            Tab::Waterfall => WaterfallDock.show(ui),
            Tab::Bookmarks => BookmarksDock.show(ui),
            Tab::Channels => ChannelsDock.show(ui),
            Tab::Demodulation => DemodulationDock.show(ui),
        }
    }

    /*fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new("dock").with(tab)
    }*/

    fn add_popup(&mut self, ui: &mut egui::Ui, path: NodePath) {
        add_tab_menu(ui, Some(path), &mut self.command_buffer);
    }
}

pub fn default_dock_state() -> DockState<Tab> {
    let mut dock_state = DockState::new(vec![Tab::Demodulation]);

    let main_surface = dock_state.main_surface_mut();

    let [_left, right] = main_surface.split(
        NodeIndex::root(),
        Split::Right,
        0.2,
        Node::leaf_with(vec![Tab::Spectrum]),
    );

    main_surface.split(
        right,
        Split::Below,
        0.2,
        Node::leaf_with(vec![Tab::Waterfall]),
    );

    dock_state
}

pub fn add_tab_menu(ui: &mut egui::Ui, path: Option<NodePath>, command_buffer: &mut CommandBuffer) {
    ui.scope(|ui| {
        // apply menu style, so this always looks like a menu
        menu_style(ui.style_mut());

        if ui.button("Baseband Spectrum").clicked() {
            command_buffer.add_dock(path, Tab::Spectrum);
        }

        if ui.button("Baseband Waterfall").clicked() {
            command_buffer.add_dock(path, Tab::Waterfall);
        }

        if ui.button("Bookmarks").clicked() {
            command_buffer.add_dock(path, Tab::Bookmarks);
        }

        if ui.button("Channels").clicked() {
            command_buffer.add_dock(path, Tab::Channels);
        }

        if ui.button("Demodulation").clicked() {
            command_buffer.add_dock(path, Tab::Demodulation);
        }
    });
}
