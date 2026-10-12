//! Help menu content: the controls reference (F1) and the user guide. Works offline.

use eframe::egui::{self, RichText};

/// Every control, grouped: (group, [(input, action)]). The single source for Help > Controls.
pub const CONTROLS: &[(&str, &[(&str, &str)])] = &[
    (
        "Mouse",
        &[
            ("Left drag", "Orbit (Preferences: turntable around the vertical axis, or trackball around the view)"),
            ("Middle drag, or Shift + left drag", "Pan"),
            ("Mouse wheel, or right drag", "Zoom"),
            ("Gizmo (top right): click an axis", "Look along that axis"),
        ],
    ),
    (
        "Keyboard: view",
        &[
            ("Ctrl + arrows", "Orbit (hold)"),
            ("Shift + arrows", "Pan (hold)"),
            ("+ / −, Page Up / Page Down", "Zoom (hold)"),
            ("Q / E", "Roll (hold)"),
            ("F", "Fit all (also stops centring on an object)"),
            ("0", "Isometric view"),
            ("1 / 2", "Front / back view"),
            ("3 / 4", "Left / right view"),
            ("5 / 6", "Top / bottom view"),
            ("W", "Wireframe on / off"),
            ("H", "Hide all objects / show them again"),
        ],
    ),
    (
        "Keyboard: files and windows",
        &[
            ("← / →", "Previous / next model file in the same folder (opened alone; preloaded, so it shows at once)"),
            ("Ctrl + O", "Open files"),
            ("Ctrl + W", "Clear the scene"),
            ("Ctrl + ,", "Preferences"),
            ("F1", "This controls overview"),
        ],
    ),
    (
        "Space mouse (3Dconnexion)",
        &[
            ("Push left / right, lift / press", "Pan"),
            ("Pull toward you / push away", "Zoom"),
            ("Tilt and spin the cap", "Orbit"),
            ("Twist left / right", "Roll"),
            ("Button 1 / button 2", "Fit all / isometric view"),
        ],
    ),
    (
        "Objects panel",
        &[
            ("Checkbox", "Show or hide an object (a model's checkbox also covers its supports)"),
            ("Crosshair icon", "Centre the view on that model and orbit around it; click again to stop"),
        ],
    ),
];

/// User guide sections: (title, paragraphs).
pub const GUIDE: &[(&str, &[&str])] = &[
    (
        "Opening files",
        &[
            "MeshPeek opens STL (binary and ASCII), OBJ, PLY and Lychee Slicer .lys files. Use File > Open (Ctrl+O), \
             File > Open Recent, drag files onto the window, or pass them on the command line.",
            "Several files can be open at once; they keep their own coordinates, so parts exported from one \
             assembly line up. File > Clear scene (Ctrl+W) removes everything.",
            "File > Open folder lists a folder in the Files tab; tick files there to show them. ZIP, 7z and RAR \
             archives open like folders: their models are read straight from the archive, nothing is unpacked \
             to disk.",
            "The left / right arrow keys open the previous / next model of the same folder or archive, alone in \
             the scene. MeshPeek loads those two neighbours in the background while you look at a file, so \
             stepping through a folder is instant (Preferences > Navigation turns this off to save memory).",
        ],
    ),
    (
        "Moving around",
        &[
            "Orbit with the left mouse button, pan with the middle button (or Shift + left), zoom with the wheel \
             or the right button. The keyboard and a 3Dconnexion space mouse work too: see Help > Controls (F1).",
            "F fits everything into view. The number keys 0–6 and the gizmo in the top-right corner switch to \
             the standard views. W toggles wireframe, H hides or shows all objects.",
        ],
    ),
    (
        "Objects panel",
        &[
            "Every model is listed with a checkbox; for Lychee files its supports appear indented under it. \
             Hiding a model hides its supports as well; supports can also be hidden on their own.",
            "Click the crosshair icon next to a model to centre the view on it: rotation then pivots around that \
             model and the standard views keep it framed. Only one model can be centred at a time; click the \
             highlighted icon (or press F) to go back to the whole scene. The (i) icon shows facts about the \
             model and its file.",
            "Models that could not be loaded are listed in orange with the reason.",
        ],
    ),
    (
        "Lychee .lys files",
        &[
            "Models are placed as in Lychee, with the build plate. Supports, braces, mini supports and rafts are \
             rebuilt from the data in the file with the same geometry rules as Lychee, including every tip, \
             middle and foot shape (cone, cube, cylinder, cross, octagon…) and all raft types.",
            "Models from the Lychee Library (.lyd) are encrypted and cannot be shown; they are listed as skipped.",
        ],
    ),
    (
        "Preferences",
        &[
            "File > Preferences (Ctrl+,) is arranged in sections (Appearance, Navigation, Space mouse, \
             System) listed on the left. It sets the material and colour of models and supports (previewed live), \
             the support detail, whether OBJ files are Y-up (Blender, Maya and ZBrush export them that way), \
             the theme (light, dark or the system's), the mouse orbit style, the build plate, preloading for the \
             arrow keys and the space mouse. Changing support detail or the OBJ axis reloads the open files.",
            "View > Colour each model gives every model its own colour.",
        ],
    ),
    (
        "Graphics card and performance",
        &["MeshPeek draws with the graphics card. Without a usable one it falls back to software rendering \
             (start with --cpu to force it); Help > About shows which is in use. Very large models stay fast on \
             a graphics card; in software mode they work but move slowly. Lowering the support detail helps \
             with large supported scenes."],
    ),
];

/// The Controls window: every mouse, keyboard and space mouse action.
pub fn controls_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Controls").open(open).resizable(false).collapsible(false).show(ctx, |ui| {
        for (group, rows) in CONTROLS {
            ui.label(RichText::new(*group).strong());
            egui::Grid::new(*group).num_columns(2).spacing([24.0, 4.0]).striped(true).show(ui, |ui| {
                for (input, action) in *rows {
                    ui.label(RichText::new(*input).monospace());
                    ui.label(*action);
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
        }
    });
}

/// The User guide window.
pub fn guide_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("User guide").open(open).default_width(520.0).default_height(480.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (title, paragraphs) in GUIDE {
                ui.heading(*title);
                for paragraph in *paragraphs {
                    ui.label(*paragraph);
                    ui.add_space(4.0);
                }
                ui.add_space(8.0);
            }
        });
    });
}
