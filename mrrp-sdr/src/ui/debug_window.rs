use crate::{
    ui::app::AppState,
    util::{
        build_info::BUILD_INFO,
        github_urls::GithubUrls,
    },
};

#[derive(Debug)]
pub struct DebugWindow<'a> {
    app_state: &'a mut AppState,
}

impl<'a> DebugWindow<'a> {
    pub fn new(app_state: &'a mut AppState) -> Self {
        Self { app_state }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("debug_window");

        let mut debug_on_hover = ctx.debug_on_hover();

        egui::Window::new("About mrrp-sdr")
            .id(id.with("window"))
            .vscroll(true)
            .hscroll(true)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .collapsible(true)
            .open(&mut self.app_state.show_debug_window)
            .show(ctx, |ui| {
                ui.collapsing("Settings", |ui| {
                    ctx.settings_ui(ui);

                    ui.checkbox(
                        &mut self.app_state.persist_everything,
                        "Persist all AppState",
                    );
                });

                ui.collapsing("Inspection", |ui| {
                    ctx.inspection_ui(ui);

                    ui.checkbox(&mut debug_on_hover, "Debug on Hover");
                });

                ui.collapsing("Memory", |ui| {
                    ctx.memory_ui(ui);
                });

                ui.collapsing("Build", |ui| {
                    ui.small("Target:");
                    ui.monospace(BUILD_INFO.target);
                    ui.small("Opt Level:");
                    ui.monospace(BUILD_INFO.opt_level);
                    ui.small("Debug:");
                    ui.monospace(BUILD_INFO.debug);
                    ui.small("Profile:");
                    ui.monospace(BUILD_INFO.profile);
                    if let Some(branch) = BUILD_INFO.git_branch {
                        ui.small("Branch:");
                        ui.hyperlink_to(
                            egui::WidgetText::from(branch).monospace(),
                            GithubUrls::PACKAGE.branch(branch),
                        );
                    }

                    if let Some(commit) = BUILD_INFO.git_commit {
                        ui.small("Commit:");
                        ui.hyperlink_to(
                            egui::WidgetText::from(commit).monospace(),
                            GithubUrls::PACKAGE.commit(commit),
                        );
                    }
                });
            });

        ctx.set_debug_on_hover(debug_on_hover);
    }
}
