use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;

use log::{debug, error};
use rmpv::Value;

use crate::nvim_bridge::{Message, Request};
use crate::nvim_gio::GioNeovim;
use crate::ui::cmdline::Cmdline;
use crate::ui::color::{Highlight, HlDefs};
use crate::ui::common::spawn_local;
#[cfg(feature = "libwebkit2gtk")]
use crate::ui::cursor_tooltip::CursorTooltip;
use crate::ui::font::Font;
use crate::ui::grid::Grid;
use crate::ui::popupmenu::Popupmenu;
use crate::ui::state::{attach_grid_events, UIState, Windows};
use crate::ui::tabline::Tabline;
use crate::ui::window::MsgWindow;

enum DropTargetType {
    Utf8String,
    TargetString,
    TextUriList,
    TextPlain,
}

macro_rules! create_target {
    ($name: expr, $id: expr) => {
        gtk::TargetEntry::new($name, gtk::TargetFlags::OTHER_APP, $id)
    };
}

/// Main UI structure.
pub struct UI {
    /// Main window.
    win: gtk::ApplicationWindow,
    /// Neovim instance.
    nvim: GioNeovim,
    /// Channel to receive event from nvim.
    rx: glib::Receiver<Message>,
    /// Our internal state, containing basically everything we manipulate
    /// when we receive an event from nvim.
    state: Rc<RefCell<UIState>>,
}

impl UI {
    /// Creates new UI.
    ///
    /// * `app` - GTK application for the UI.
    /// * `rx` - Channel to receive nvim UI events.
    /// * `nvim` - Neovim instance to use. Should be the same that is the source
    ///            of `rx` events.
    pub fn init(
        app: &gtk::Application,
        rx: glib::Receiver<Message>,
        window_size: (i32, i32),
        nvim: GioNeovim,
    ) -> Self {
        // Create the main window.
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("Neovim");
        window.set_default_size(window_size.0, window_size.1);

        // Realize window resources.
        window.realize();

        // Top level widget.
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.add(&b);

        let tabline = Tabline::new(nvim.clone());
        b.pack_start(&tabline.get_widget(), false, false, 0);

        // Our root widget for all grids/windows.
        let overlay = gtk::Overlay::new();
        b.pack_start(&overlay, true, true, 0);

        // Create hl defs and initialize 0th element because we'll need to have
        // something that is accessible for the default grid that we're gonna
        // make next.
        let mut hl_defs = HlDefs::default();
        hl_defs.insert(0, Highlight::default());

        let font = Font::from_guifont("Monospace:h12").unwrap();
        let line_space = 0;

        // Create default grid.
        let mut grid = Grid::new(
            1,
            &window.get_window().unwrap(),
            font.clone(),
            line_space,
            80,
            30,
            &hl_defs,
            true,
        );
        // Mark the default grid as active at the beginning.
        grid.set_active(true);
        overlay.add(&grid.widget());

        let windows_container = gtk::Fixed::new();
        windows_container.set_widget_name("windows-contianer");
        let windows_float_container = gtk::Fixed::new();
        windows_float_container.set_widget_name("windows-contianer-float");
        let msg_window_container = gtk::Fixed::new();
        msg_window_container.set_widget_name("message-grid-contianer");
        overlay.add_overlay(&windows_container);
        overlay.add_overlay(&msg_window_container);
        overlay.add_overlay(&windows_float_container);

        let css_provider = gtk::CssProvider::new();
        let msg_window =
            MsgWindow::new(msg_window_container.clone(), css_provider.clone());

        overlay.set_overlay_pass_through(&windows_container, true);
        overlay.set_overlay_pass_through(&windows_float_container, true);
        overlay.set_overlay_pass_through(&msg_window_container, true);

        // When resizing our window (main grid), we'll have to tell neovim to
        // resize it self also. The notify to nvim is send with a small delay,
        // so we don't spam it multiple times a second. source_id is used to
        // track the function timeout. This timeout might be canceled in
        // redraw even handler if we receive a message that changes the size
        // of the main grid.
        let source_id = Rc::new(RefCell::new(None));
        grid.connect_da_resize(clone!(nvim, source_id => move |rows, cols| {

            // Set timeout to notify nvim about the new size.
            let new = gtk::timeout_add(30, clone!(nvim, source_id => move || {
                let nvim = nvim.clone();
                spawn_local(async move {
                    if let Err(err) = nvim.ui_try_resize(cols as i64, rows as i64).await {
                        error!("Error: failed to resize nvim when grid size changed ({:?})", err);
                    }
                });

                // Set the source_id to none, so we don't accidentally remove
                // it since it used at this point.
                source_id.borrow_mut().take();

                Continue(false)
            }));

            let mut source_id = source_id.borrow_mut();
            // If we have earlier timeout, remove it.
            if let Some(old) = source_id.take() {
                glib::source::source_remove(old);
            }

            *source_id = Some(new);

            false
        }));

        attach_grid_events(&grid, nvim.clone());

        // IMMulticontext is used to handle most of the inputs.
        let im_context = gtk::IMMulticontext::new();
        im_context.set_use_preedit(false);
        im_context.connect_commit(clone!(nvim => move |_, input| {
            // "<" needs to be escaped for nvim.input()
            let nvim_input = input.replace("<", "<lt>");

            let nvim = nvim.clone();
            spawn_local(async move {
                nvim.input(&nvim_input).await.expect("Couldn't send input");
            });
        }));

        window.connect_key_press_event(clone!(nvim, im_context => move |_, e| {
            if im_context.filter_keypress(e) {
                Inhibit(true)
            } else {
                if let Some(input) = event_to_nvim_input(e) {
                    let nvim = nvim.clone();
                    spawn_local(async move {
                        nvim.input(input.as_str()).await.expect("Couldn't send input");
                    });
                    return Inhibit(true);
                } else {
                    debug!(
                        "Failed to turn input event into nvim key (keyval: {})",
                        e.get_keyval()
                    )
                }

                Inhibit(false)
            }
        }));

        window.connect_key_release_event(clone!(im_context => move |_, e| {
            im_context.filter_keypress(e);
            Inhibit(false)
        }));

        window.connect_focus_in_event(clone!(im_context => move |_, _| {
            im_context.focus_in();
            Inhibit(false)
        }));

        window.connect_focus_out_event(clone!(im_context => move |_, _| {
            im_context.focus_out();
            Inhibit(false)
        }));

        // Note: Order matters, preferred types should come first.
        let targets = &[
            create_target!("text/uri-list", DropTargetType::TextUriList as u32),
            create_target!("UTF8_STRING", DropTargetType::Utf8String as u32),
            create_target!("STRING", DropTargetType::TargetString as u32),
            create_target!("text/plain", DropTargetType::TextPlain as u32),
        ];

        // clear drag_dest configs
        window.drag_dest_unset();
        window.drag_dest_set(
            gtk::DestDefaults::ALL,
            targets,
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );

        window.connect_drag_data_received(clone!(nvim =>
        move |_, _, _x, _y, data, info, _| {
            if info == DropTargetType::TextUriList as u32 {
                handle_drop_uri_list(nvim.clone(), data);
            } else if let Some(text) = data.get_text() {
                let nvim = nvim.clone();
                spawn_local(async move {
                    // the result from paste command is ignored
                    nvim.paste(&text, true, -1).await
                        .expect("nvim_paste command did not return the correct type");
                });
            }
        }));

        let cmdline = Cmdline::new(&overlay, nvim.clone());
        #[cfg(feature = "libwebkit2gtk")]
        let cursor_tooltip = CursorTooltip::new(&overlay);

        window.show_all();

        grid.set_im_context(&im_context);

        cmdline.hide();
        #[cfg(feature = "libwebkit2gtk")]
        cursor_tooltip.hide();

        let mut grids = HashMap::new();
        grids.insert(1, grid);

        add_css_provider!(&css_provider, window);

        UI {
            win: window,
            rx,
            state: Rc::new(RefCell::new(UIState {
                css_provider,
                windows: Windows::new(),
                windows_container,
                msg_window_container,
                msg_window,
                windows_float_container,
                grids,
                mode_infos: vec![],
                current_grid: 1,
                wildmenu_shown: false,
                popupmenu: Popupmenu::new(&overlay, nvim.clone()),
                cmdline,
                overlay,
                tabline,
                #[cfg(feature = "libwebkit2gtk")]
                cursor_tooltip,
                resize_source_id: source_id,
                hl_defs,
                resize_on_flush: None,
                hl_changed: false,
                font,
                line_space,
                current_mode: None,
                enable_cursor_animations: true,
            })),
            nvim,
        }
    }

    /// Starts to listen events from `rx` (e.g. from nvim) and processing those.
    /// Think this as the "main" function of the UI.
    pub fn start(self) {
        let UI {
            rx,
            state,
            win,
            nvim,
        } = self;

        rx.attach(None, move |message| {
            match message {
                // Handle a notify.
                Message::Notify(notify) => {
                    let mut state = state.borrow_mut();

                    state.handle_notify(&win, notify, &nvim);
                }
                // Handle a request.
                Message::Request(tx, request) => {
                    let mut state = state.borrow_mut();
                    let res = handle_request(&request, &mut state);
                    tx.send(res).expect("Failed to respond to a request");
                }
                // Handle close.
                Message::Close => {
                    win.close();
                    return Continue(false);
                }
            }

            Continue(true)
        });
    }
}

#[cfg_attr(not(feature = "libwebkit2gtk"), allow(unused_variables))] // Silence clippy
fn handle_request(
    request: &Request,
    state: &mut UIState,
) -> Result<Value, Value> {
    match request {
        #[cfg(feature = "libwebkit2gtk")]
        Request::CursorTooltipStyles => {
            let styles = state.cursor_tooltip.get_styles();

            let res: Vec<Value> =
                styles.into_iter().map(|s| s.into()).collect();

            Ok(res.into())
        }
        #[cfg(not(feature = "libwebkit2gtk"))]
        Request::CursorTooltipStyles => {
            Err("Cursor tooltip is not supported in this build".into())
        }
    }
}

fn keyname_to_nvim_key(s: &str) -> Option<&str> {
    // Originally sourced from python-gui.
    match s {
        "asciicircum" => Some("^"), // fix #137
        "slash" => Some("/"),
        "backslash" => Some("\\"),
        "dead_circumflex" => Some("^"),
        "at" => Some("@"),
        "numbersign" => Some("#"),
        "dollar" => Some("$"),
        "percent" => Some("%"),
        "ampersand" => Some("&"),
        "asterisk" => Some("*"),
        "parenleft" => Some("("),
        "parenright" => Some(")"),
        "underscore" => Some("_"),
        "plus" => Some("+"),
        "minus" => Some("-"),
        "bracketleft" => Some("["),
        "bracketright" => Some("]"),
        "braceleft" => Some("{"),
        "braceright" => Some("}"),
        "dead_diaeresis" => Some("\""),
        "dead_acute" => Some("\'"),
        "less" => Some("<"),
        "greater" => Some(">"),
        "comma" => Some(","),
        "period" => Some("."),
        "BackSpace" => Some("BS"),
        "Insert" => Some("Insert"),
        "Return" => Some("CR"),
        "Escape" => Some("Esc"),
        "Delete" => Some("Del"),
        "Page_Up" => Some("PageUp"),
        "Page_Down" => Some("PageDown"),
        "Enter" => Some("CR"),
        "ISO_Left_Tab" => Some("Tab"),
        "Tab" => Some("Tab"),
        "Up" => Some("Up"),
        "Down" => Some("Down"),
        "Left" => Some("Left"),
        "Right" => Some("Right"),
        "Home" => Some("Home"),
        "End" => Some("End"),
        "F1" => Some("F1"),
        "F2" => Some("F2"),
        "F3" => Some("F3"),
        "F4" => Some("F4"),
        "F5" => Some("F5"),
        "F6" => Some("F6"),
        "F7" => Some("F7"),
        "F8" => Some("F8"),
        "F9" => Some("F9"),
        "F10" => Some("F10"),
        "F11" => Some("F11"),
        "F12" => Some("F12"),
        _ => None,
    }
}

fn event_to_nvim_input(e: &gdk::EventKey) -> Option<String> {
    let mut input = String::from("");

    let keyval = e.get_keyval();
    let keyname = keyval.name()?;

    let state = e.get_state();

    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        input.push_str("S-");
    }
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        input.push_str("C-");
    }
    if state.contains(gdk::ModifierType::MOD1_MASK) {
        input.push_str("A-");
    }

    if keyname.chars().count() > 1 {
        let n = keyname_to_nvim_key(keyname.as_str())?;
        input.push_str(n);
    } else {
        input.push(keyval.to_unicode()?);
    }

    Some(format!("<{}>", input))
}

fn handle_drop_uri_list(nvim: GioNeovim, data: &gtk::SelectionData) {
    let filenames = data.get_uris();

    spawn_local(async move {
        for filename in filenames {
            if let Ok(filename) = urlencoding::decode(filename.as_str()) {
                // errors from `command` are nvim command errors and will be
                // shown to the user except for E37, so we ignore handling them here
                //
                // first try to edit the file, if failed due to the current buffer
                // is modified, edit the file in a new split
                if let Err(e) = nvim.command(&format!("e {}", filename)).await {
                    if let nvim_rs::error::CallError::NeovimError(_, s) =
                        e.as_ref()
                    {
                        // only respond to
                        // "Vim(edit):E37: No write since last change (add ! to override)"
                        if s.starts_with("Vim(edit):E37") {
                            // ignore errors
                            nvim.command(&format!("sp {}", filename))
                                .await
                                .ok();
                        }
                    }
                }
            }
        }
    });
}
