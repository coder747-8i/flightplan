//! The one modal dialog that can be open at a time.

use crate::app::{App, Dialog};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let keep = match &mut d {
        Dialog::Export => super::export::show(app, ctx),
        Dialog::Wizard(w) => super::wizard::show(app, ctx, w),
        Dialog::Import(i) => super::import::show(app, ctx, i),
    };
    // a dialog may have opened another one (the wizard's "Import a file")
    if keep && app.dialog.is_none() {
        app.dialog = Some(d);
    }
}
