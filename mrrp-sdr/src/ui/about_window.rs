use crate::{
    ui::state::AppState,
    util::{
        build_info::BUILD_INFO,
        github_urls::GithubUrls,
    },
};

#[derive(Debug)]
pub struct AboutWindow<'a> {
    app_state: &'a mut AppState,
}

impl<'a> AboutWindow<'a> {
    pub fn new(app_state: &'a mut AppState) -> Self {
        Self { app_state }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("about_window");

        egui::Window::new("About mrrp-sdr")
            .id(id.with("window"))
            .vscroll(true)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .collapsible(false)
            .open(&mut self.app_state.show_about_window)
            .show(ctx, |ui| {
                ui.heading(format!(
                    "{} {}",
                    std::env!("CARGO_PKG_NAME"),
                    std::env!("CARGO_PKG_VERSION"),
                ));

                ui.label(format!(
                    "This is {} {}",
                    std::env!("CARGO_PKG_NAME"),
                    std::env!("CARGO_PKG_VERSION"),
                ));

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    ui.label(format!(
                        "This version was built at {}",
                        BUILD_INFO.build_time
                    ));

                    if let (Some(commit), Some(branch)) =
                        (BUILD_INFO.git_commit, BUILD_INFO.git_branch)
                    {
                        ui.label(" from commit ");
                        ui.monospace(commit);

                        ui.label(" and branch ");
                        ui.monospace(branch);
                    }

                    ui.label(".");
                });

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    ui.label("Website: ");

                    let url = GithubUrls::PACKAGE.repository;

                    ui.add(egui::Hyperlink::from_label_and_url(
                        egui::RichText::new(&*url).text_style(egui::TextStyle::Monospace),
                        &url,
                    ));
                });

                ui.separator();

                ui.heading("Attributions");

                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to("DSEG font", "https://github.com/keshikan/DSEG");
                    ui.label("by keshikan licensed under");
                    ui.hyperlink_to(
                        "SIL Open Font License Version 1.1",
                        "https://github.com/keshikan/DSEG/blob/master/DSEG-LICENSE.txt",
                    );
                })
            });
    }
}
