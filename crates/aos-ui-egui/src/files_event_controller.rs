//! Réception des événements `fs.*` (S5).

use crate::cmd::Cmd;
use crate::UiApp;
use aos_proto::{DataClass, FsEntry};

pub(crate) fn on_listed(app: &mut UiApp, entries: Vec<FsEntry>) {
    app.files_ui.entries = entries;
}

pub(crate) fn on_read(
    app: &mut UiApp,
    path: String,
    content: String,
    class: DataClass,
    version: u64,
) {
    // Renommage en attente : le contenu vient d'arriver → write + delete,
    // et le viewer suit la destination.
    if let Some((from, to)) = app.files_ui.pending_rename.clone() {
        if from == path {
            app.files_ui.pending_rename = None;
            let _ = app.cmd_tx.send(Cmd::FilesWrite {
                path: to.clone(),
                content: content.clone(),
            });
            let _ = app.cmd_tx.send(Cmd::FilesDelete { path: path.clone() });
            app.files_ui.open_path = Some(to);
            app.files_ui.open_content = content;
            app.files_ui.open_class = class;
            app.files_ui.open_version = version;
            app.files_ui.open_dirty = false;
            return;
        }
    }
    app.files_ui.open_path = Some(path);
    app.files_ui.open_content = content;
    app.files_ui.open_class = class;
    app.files_ui.open_version = version;
    app.files_ui.open_dirty = false;
}

pub(crate) fn on_op_ok(app: &mut UiApp, msg: String) {
    app.push_status(msg.clone());
    app.toasts.push_success(msg);
    // Re-liste après chaque mutation (write/delete/set_class).
    let _ = app.cmd_tx.send(Cmd::FilesList {
        prefix: String::new(),
    });
}
