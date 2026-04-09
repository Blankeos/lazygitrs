use anyhow::Result;

use crate::gui::popup::{MenuItem, PopupState};
use crate::gui::Gui;

/// Handle bisect options from the commits panel.
pub fn show_bisect_menu(gui: &mut Gui) -> Result<()> {
    let is_bisecting = gui.git.is_bisecting();

    if is_bisecting {
        show_bisect_in_progress_menu(gui)
    } else {
        show_bisect_start_menu(gui)
    }
}

fn show_bisect_start_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    let hash = model
        .commits
        .get(selected)
        .map(|c| c.hash.clone())
        .unwrap_or_default();
    drop(model);

    let h1 = hash.clone();
    let h2 = hash.clone();

    gui.popup = PopupState::Menu {
        title: "Bisect".to_string(),
        items: vec![
            MenuItem {
                label: "Start bisect, mark current as bad".to_string(),
                description: "Begin bisect session".to_string(),
                key: Some("b".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.bisect_start()?;
                    gui.git.bisect_bad("")?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Mark selected commit as good".to_string(),
                description: "Start bisect and mark this commit as good".to_string(),
                key: Some("g".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.bisect_start()?;
                    gui.git.bisect_good(&h1)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Mark selected commit as bad".to_string(),
                description: "Start bisect and mark this commit as bad".to_string(),
                key: Some("B".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.bisect_start()?;
                    gui.git.bisect_bad(&h2)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
        ],
        selected: 0,
        loading_index: None,
    };
    Ok(())
}

fn show_bisect_in_progress_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    let hash = model
        .commits
        .get(selected)
        .map(|c| c.hash.clone())
        .unwrap_or_default();
    drop(model);

    let h1 = hash.clone();
    let h2 = hash.clone();
    let h3 = hash.clone();

    gui.popup = PopupState::Menu {
        title: "Bisect (in progress)".to_string(),
        items: vec![
            MenuItem {
                label: "Mark current as good".to_string(),
                description: "Bisect good".to_string(),
                key: Some("g".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.bisect_good(&h1)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Mark current as bad".to_string(),
                description: "Bisect bad".to_string(),
                key: Some("b".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.bisect_bad(&h2)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Skip current".to_string(),
                description: "Bisect skip".to_string(),
                key: Some("s".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.bisect_skip(&h3)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Reset bisect".to_string(),
                description: "End bisect session".to_string(),
                key: Some("r".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.bisect_reset()?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
        ],
        selected: 0,
        loading_index: None,
    };
    Ok(())
}
