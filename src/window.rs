// SPDX-FileCopyrightText: 2022  Emmanuele Bassi
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::subclass::prelude::*;
use glib::{clone, closure_local, Receiver};
use gtk::{gdk, gio, glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::{
    audio::{AudioPlayer, RepeatMode, Song, WaveformGenerator},
    config::APPLICATION_ID,
    drag_overlay::DragOverlay,
    i18n::{i18n, ni18n_f},
    playback_control::PlaybackControl,
    queue_row::QueueRow,
    song_details::SongDetails,
    utils,
    waveform_view::WaveformView,
};

pub enum WindowAction {
    Present,
}

mod imp {
    use glib::{ParamFlags, ParamSpec, ParamSpecBoolean, Value};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(CompositeTemplate)]
    #[template(resource = "/io/bassi/Amberol/window.ui")]
    pub struct Window {
        // Template widgets
        #[template_child]
        pub drag_overlay: TemplateChild<DragOverlay>,
        #[template_child]
        pub back_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub status_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub song_details: TemplateChild<SongDetails>,
        #[template_child]
        pub playback_control: TemplateChild<PlaybackControl>,
        #[template_child]
        pub queue_revealer: TemplateChild<adw::Flap>,
        #[template_child]
        pub queue_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub queue_length_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub playlist_box: TemplateChild<gtk::Box>,

        pub player: Rc<AudioPlayer>,
        pub provider: gtk::CssProvider,
        pub receiver: RefCell<Option<Receiver<WindowAction>>>,
        pub waveform: WaveformGenerator,

        pub playlist_shuffled: Cell<bool>,
        pub playlist_visible: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "AmberolWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("win.play", None, move |win, _, _| {
                debug!("Window::win.play()");
                win.imp().player.toggle_play();
            });
            klass.install_action("win.previous", None, move |win, _, _| {
                debug!("Window::win.previous()");
                win.imp().player.skip_previous();
            });
            klass.install_action("win.next", None, move |win, _, _| {
                debug!("Window::win.next()");
                win.imp().player.skip_next();
            });
            klass.install_action("win.seek-backwards", None, move |win, _, _| {
                debug!("Window::win.seek-backwards()");
                win.imp().player.seek_backwards();
            });
            klass.install_action("win.seek-forward", None, move |win, _, _| {
                debug!("Window::win.seek-forward()");
                win.imp().player.seek_forward();
            });
            klass.install_action("queue.repeat-mode", None, move |win, _, _| {
                debug!("Window::queue.repeat()");
                win.imp().player.toggle_repeat_mode();
            });
            klass.install_action("queue.add-song", None, move |win, _, _| {
                debug!("Window::win.add-song()");
                win.add_song();
            });
            klass.install_action("queue.add-folder", None, move |win, _, _| {
                debug!("Window::win.add-folder()");
                win.add_folder();
            });
            klass.install_action("queue.clear", None, move |win, _, _| {
                debug!("Window::queue.clear()");
                win.clear_queue();
            });
            klass.install_property_action("queue.toggle", "playlist-visible");
            klass.install_property_action("queue.shuffle", "playlist-shuffled");
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            let (sender, r) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
            let receiver = RefCell::new(Some(r));

            Self {
                song_details: TemplateChild::default(),
                queue_revealer: TemplateChild::default(),
                queue_view: TemplateChild::default(),
                queue_length_label: TemplateChild::default(),
                drag_overlay: TemplateChild::default(),
                playback_control: TemplateChild::default(),
                main_stack: TemplateChild::default(),
                status_page: TemplateChild::default(),
                back_button: TemplateChild::default(),
                playlist_box: TemplateChild::default(),
                playlist_shuffled: Cell::new(false),
                playlist_visible: Cell::new(true),
                player: AudioPlayer::new(sender),
                waveform: WaveformGenerator::default(),
                provider: gtk::CssProvider::new(),
                receiver,
            }
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            if APPLICATION_ID.ends_with("Devel") {
                obj.add_css_class("devel");
            }

            obj.setup_channel();
            obj.setup_waveform();
            obj.set_initial_state();
            obj.bind_state();
            obj.bind_queue();
            obj.connect_signals();
            obj.setup_playlist();
            obj.setup_drop_target();
            obj.setup_provider();
            obj.restore_window_state();
        }

        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecBoolean::new(
                        "playlist-shuffled",
                        "",
                        "",
                        false,
                        ParamFlags::READWRITE,
                    ),
                    ParamSpecBoolean::new("playlist-visible", "", "", false, ParamFlags::READWRITE),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, obj: &Self::Type, _id: usize, value: &Value, pspec: &ParamSpec) {
            match pspec.name() {
                "playlist-shuffled" => obj.set_playlist_shuffled(value.get::<bool>().unwrap()),
                "playlist-visible" => obj.set_playlist_visible(value.get::<bool>().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &ParamSpec) -> Value {
            match pspec.name() {
                "playlist-shuffled" => obj.playlist_shuffled().to_value(),
                "playlist-visible" => obj.playlist_visible().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Window {
    pub fn new<P: glib::IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::new(&[("application", application)]).expect("Failed to create Window")
    }

    fn setup_channel(&self) {
        let receiver = self.imp().receiver.borrow_mut().take().unwrap();
        receiver.attach(
            None,
            clone!(@strong self as this => move |action| this.process_action(action)),
        );
    }

    fn process_action(&self, action: WindowAction) -> glib::Continue {
        match action {
            WindowAction::Present => self.present(),
            // _ => debug!("Received action {:?}", action),
        }

        glib::Continue(true)
    }

    fn setup_waveform(&self) {
        self.imp().waveform.connect_notify_local(
            Some("has-peaks"),
            clone!(@strong self as win => move |gen, _| {
                let peaks = gen.peaks();
                win.imp().playback_control.waveform_view().set_peaks(peaks);
            }),
        );
    }

    fn restore_window_state(&self) {
        // FIXME: https://gitlab.gnome.org/GNOME/gtk/-/issues/4136
        // let settings = utils::settings_manager();
        // let width = settings.int("window-width");
        // let height = settings.int("window-height");
        // self.set_default_size(width, height);
        self.set_default_size(600, -1);
    }

    fn clear_queue(&self) {
        let player = &self.imp().player;
        player.stop();
        player.state().set_current_song(None);
        player.queue().clear();
        self.imp().queue_revealer.set_reveal_flap(false);
    }

    fn playlist_visible(&self) -> bool {
        self.imp().playlist_visible.get()
    }

    fn set_playlist_visible(&self, visible: bool) {
        if visible != self.imp().playlist_visible.replace(visible) {
            self.imp().queue_revealer.set_reveal_flap(visible);
            self.notify("playlist-visible");
        }
    }

    fn playlist_shuffled(&self) -> bool {
        self.imp().playlist_shuffled.get()
    }

    fn set_playlist_shuffled(&self, shuffled: bool) {
        let imp = self.imp();

        if shuffled != imp.playlist_shuffled.replace(shuffled) {
            let queue = imp.player.queue();
            let state = imp.player.state();

            let reset_song = queue.is_first_song() && !state.playing();

            queue.set_shuffle(shuffled);

            if reset_song {
                imp.player.skip_to(0);
            }

            self.notify("playlist-shuffled");
        }
    }

    fn add_song(&self) {
        let app = gio::Application::default()
            .expect("Failed to retrieve application singleton")
            .downcast::<gtk::Application>()
            .unwrap();
        let win = app
            .active_window()
            .unwrap()
            .downcast::<gtk::Window>()
            .unwrap();
        let dialog = gtk::FileChooserNative::builder()
            .accept_label(&i18n("_Add Song"))
            .cancel_label(&i18n("_Cancel"))
            .modal(true)
            .title("Open File")
            .action(gtk::FileChooserAction::Open)
            .transient_for(&win)
            .build();

        let filter = gtk::FileFilter::new();
        gtk::FileFilter::set_name(&filter, Some(&i18n("Audio files")));
        filter.add_mime_type("audio/*");
        dialog.add_filter(&filter);

        dialog.connect_response(
            clone!(@strong dialog, @weak self as win => move |_, response| {
                if response == gtk::ResponseType::Accept {
                    win.add_files_to_queue(&dialog.files());
                }
            }),
        );
        dialog.show();
    }

    fn add_folder(&self) {
        let app = gio::Application::default()
            .expect("Failed to retrieve application singleton")
            .downcast::<gtk::Application>()
            .unwrap();
        let win = app
            .active_window()
            .unwrap()
            .downcast::<gtk::Window>()
            .unwrap();
        let dialog = gtk::FileChooserNative::builder()
            .accept_label(&i18n("_Add Folder"))
            .cancel_label(&i18n("_Cancel"))
            .modal(true)
            .title("Open Folder")
            .action(gtk::FileChooserAction::SelectFolder)
            .transient_for(&win)
            .build();

        dialog.connect_response(
            clone!(@strong dialog, @weak self as win => move |_, response| {
                if response == gtk::ResponseType::Accept {
                    win.add_folders_to_queue(&dialog.files());
                }
            }),
        );
        dialog.show();
    }

    fn add_folders_to_queue(&self, folders: &gio::ListModel) {
        for pos in 0..folders.n_items() {
            let folder = folders.item(pos).unwrap().downcast::<gio::File>().unwrap();
            self.add_folder_to_queue(&folder);
        }
    }

    fn add_files_to_queue(&self, files: &gio::ListModel) {
        for pos in 0..files.n_items() {
            let file = files.item(pos).unwrap().downcast::<gio::File>().unwrap();
            debug!("Adding {} to the queue", file.uri());
            self.add_file_to_queue(&file);
        }
    }

    pub fn add_file_to_queue(&self, file: &gio::File) {
        let queue = self.imp().player.queue();
        let was_empty = queue.is_empty();

        let song = Song::new(&file.uri());
        queue.add_song(&song);

        if was_empty {
            self.imp().player.skip_to(0);
        }
    }

    pub fn add_folder_to_queue(&self, folder: &gio::File) {
        debug!("Adding the contents of {} to the queue", folder.uri());

        let mut files = utils::load_files_from_folder(folder, true).into_iter();
        glib::idle_add_local(clone!(@strong self as win => move || {
            let queue = win.imp().player.queue();

            files.next()
                .map(|f| {
                    let s = Song::new(f.uri().as_str());
                    if !s.equals(&Song::default()) {
                        let was_empty = queue.is_empty();
                        queue.add_song(&s);
                        if was_empty {
                            win.imp().player.skip_to(0);
                        }
                    }
                })
                .map(|_| glib::Continue(true))
                .unwrap_or_else(|| {
                    glib::Continue(false)
                })
        }));
    }

    // Bind the PlayerState to the UI
    fn bind_state(&self) {
        let imp = self.imp();
        let state = imp.player.state();

        // Use the PlayerState:playing property to control the play/pause button
        state.connect_notify_local(
            Some("playing"),
            clone!(@weak self as win => move |state, _| {
                let play_button = win.imp().playback_control.play_button();
                if state.playing() {
                    play_button.set_icon_name("media-playback-pause-symbolic");
                } else {
                    play_button.set_icon_name("media-playback-start-symbolic");
                }
            }),
        );
        // Update the position label
        state.connect_notify_local(
            Some("position"),
            clone!(@weak self as win => move |state, _| {
                if state.current_song().is_some() {
                    let position = state.position();
                    let duration = state.duration();
                    let time = utils::format_time(position, duration);
                    win.imp().song_details.time_label().set_label(&time);
                    let pos = position as f64 / duration as f64;
                    win.imp().playback_control.waveform_view().set_position(pos);
                } else {
                    win.imp().song_details.time_label().set_label("");
                }
            }),
        );
        // Update the playlist time
        state.connect_notify_local(
            Some("song"),
            clone!(@weak self as win => move |state, _| {
                win.scroll_playlist_to_song();
                win.update_playlist_time();
                if let Some(current) = state.current_song() {
                    debug!("Updating waveform for {:?}", &current);
                    win.update_waveform(Some(&current));
                    debug!("Updating style for {:?}", &current);
                    win.update_style(&current);
                } else {
                    win.update_waveform(None);
                    debug!("Reset album art");
                    win.remove_css_class("main-window");
                }
            }),
        );
        // Update the cover, if any is available
        state.connect_notify_local(
            Some("cover"),
            clone!(@weak self as win => move |state, _| {
                let song_details = win.imp().song_details.get();
                if let Some(cover) = state.cover() {
                    song_details.album_image().set_cover(Some(&cover));
                    song_details.show_cover_image(true);
                } else {
                    song_details.album_image().set_cover(None);
                    song_details.show_cover_image(false);
                }
            }),
        );
        // Bind the song properties to the UI
        state
            .bind_property("title", &imp.song_details.get().title_label(), "label")
            .flags(glib::BindingFlags::DEFAULT)
            .build();
        state
            .bind_property("artist", &imp.song_details.get().artist_label(), "label")
            .flags(glib::BindingFlags::DEFAULT)
            .build();
        state
            .bind_property("album", &imp.song_details.get().album_label(), "label")
            .flags(glib::BindingFlags::DEFAULT)
            .build();
        state
            .bind_property(
                "volume",
                &imp.playback_control.get().volume_control(),
                "volume",
            )
            .flags(glib::BindingFlags::DEFAULT)
            .build();
    }

    // Bind the Queue to the UI
    fn bind_queue(&self) {
        let queue = self.imp().player.queue();

        queue.connect_notify_local(
            Some("n-songs"),
            clone!(@weak self as win => move |queue, _| {
                if queue.is_empty() {
                    win.set_initial_state();
                } else {
                    win.imp().main_stack.get().set_visible_child_name("main-view");

                    win.action_set_enabled("queue.toggle", queue.n_songs() > 1);
                    win.action_set_enabled("queue.shuffle", queue.n_songs() > 1);

                    win.action_set_enabled("win.play", true);
                    win.action_set_enabled("win.seek-backwards", true);
                    win.action_set_enabled("win.seek-forward", true);
                    win.action_set_enabled("win.previous", true);
                    win.action_set_enabled("win.next", queue.n_songs() > 1);
                }

                if queue.n_songs() == 1 {
                    win.imp().player.skip_next();
                }

                win.update_playlist_time();
            }),
        );
        queue.connect_notify_local(
            Some("repeat-mode"),
            clone!(@weak self as win => move |queue, _| {
                let imp = win.imp();
                let repeat_button = imp.playback_control.repeat_button();
                match queue.repeat_mode() {
                    RepeatMode::Consecutive => {
                        repeat_button.set_icon_name("media-playlist-consecutive-symbolic");
                    },
                    RepeatMode::RepeatAll => {
                        repeat_button.set_icon_name("media-playlist-repeat-symbolic");
                    },
                    RepeatMode::RepeatOne => {
                        repeat_button.set_icon_name("media-playlist-repeat-song-symbolic");
                    },
                }
            }),
        );
        queue.connect_notify_local(
            Some("current"),
            clone!(@weak self as win => move |queue, _| {
                if queue.is_last_song() {
                    win.action_set_enabled("win.next", false);
                } else {
                    win.action_set_enabled("win.next", true);
                }
            }),
        );
    }

    fn connect_signals(&self) {
        self.imp().queue_revealer.connect_notify_local(
            Some("folded"),
            clone!(@weak self as win => move |flap, _| {
                win.set_playlist_visible(flap.reveals_flap());
                if flap.is_folded() {
                    win.imp().back_button.set_visible(win.playlist_visible());
                    win.imp().playlist_box.add_css_class("playlist-background");
                } else {
                    win.imp().back_button.set_visible(false);
                    win.imp().playlist_box.remove_css_class("playlist-background");
                }
            }),
        );

        self.imp().queue_revealer.connect_notify_local(
            Some("reveal-flap"),
            clone!(@weak self as win => move |flap, _| {
                win.set_playlist_visible(flap.reveals_flap());
                if flap.is_folded() {
                    win.imp().back_button.set_visible(win.playlist_visible());
                    win.imp().playlist_box.add_css_class("playlist-background");
                } else {
                    win.imp().back_button.set_visible(false);
                    win.imp().playlist_box.remove_css_class("playlist-background");
                }
            }),
        );

        let volume_control = self.imp().playback_control.volume_control();
        volume_control.connect_notify_local(
            Some("volume"),
            clone!(@weak self as win => move |control, _| {
                win.imp().player.set_volume(control.volume());
            }),
        );

        let waveform_view = self.imp().playback_control.waveform_view();
        waveform_view.connect_closure(
            "position-changed",
            false,
            closure_local!(@strong self as this => move |_view: WaveformView, position: f64| {
                debug!("New position: {}", position);
                this.imp().player.seek_position(position);
            }),
        );

        self.connect_close_request(move |window| {
            debug!("Saving window state");
            let width = window.default_size().0;
            let height = window.default_size().1;

            let settings = utils::settings_manager();
            settings
                .set_int("window-width", width)
                .expect("Unable to store window-width");
            settings
                .set_int("window-height", height)
                .expect("Unable to stop window-height");

            glib::signal::Inhibit(false)
        });
    }

    // The initial state of the playback actions
    fn set_initial_state(&self) {
        self.action_set_enabled("win.play", false);
        self.action_set_enabled("win.pause", false);
        self.action_set_enabled("win.previous", false);
        self.action_set_enabled("win.next", false);
        self.action_set_enabled("win.seek-backwards", false);
        self.action_set_enabled("win.seek-forward", false);

        self.action_set_enabled("queue.toggle", false);
        self.action_set_enabled("queue.shuffle", false);

        // Manually update the icon on the initial empty state
        // to avoid generating the UI definition file at build
        // time
        self.imp().status_page.set_icon_name(Some(APPLICATION_ID));
    }

    fn setup_playlist(&self) {
        let imp = self.imp();

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, list_item| {
            let row = QueueRow::default();
            list_item.set_child(Some(&row));

            list_item
                .bind_property("item", &row, "song")
                .flags(glib::BindingFlags::DEFAULT)
                .build();

            list_item
                .property_expression("item")
                .chain_property::<Song>("artist")
                .bind(&row, "song-artist", gtk::Widget::NONE);
            list_item
                .property_expression("item")
                .chain_property::<Song>("title")
                .bind(&row, "song-title", gtk::Widget::NONE);
            list_item
                .property_expression("item")
                .chain_property::<Song>("cover")
                .bind(&row, "song-cover", gtk::Widget::NONE);
            list_item
                .property_expression("item")
                .chain_property::<Song>("playing")
                .bind(&row, "playing", gtk::Widget::NONE);
        });
        imp.queue_view
            .set_factory(Some(&factory.upcast::<gtk::ListItemFactory>()));

        let queue = imp.player.queue();
        let selection = gtk::SingleSelection::new(Some(queue.model()));
        selection.set_can_unselect(false);
        selection.set_selected(gtk::INVALID_LIST_POSITION);
        imp.queue_view
            .set_model(Some(&selection.upcast::<gtk::SelectionModel>()));
        imp.queue_view
            .connect_activate(clone!(@weak self as win => move |_, pos| {
                let queue = win.imp().player.queue();
                if queue.current_song_index() != Some(pos) {
                    win.imp().player.skip_to(pos);
                    win.imp().player.play();
                }
            }));
    }

    fn setup_drop_target(&self) {
        let drop_target = gtk::DropTarget::builder()
            .name("file-drop-target")
            .actions(gdk::DragAction::COPY)
            .formats(&gdk::ContentFormats::for_type(gdk::FileList::static_type()))
            .build();

        drop_target.connect_drop(
            clone!(@weak self as win => @default-return false, move |_, value, _, _| {
                if let Ok(file_list) = value.get::<gdk::FileList>() {
                    for f in file_list.files() {
                        if let Ok(info) = f.query_info("standard::*", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) {
                            if info.file_type() == gio::FileType::Regular {
                                win.add_file_to_queue(&f);
                            } else if info.file_type() == gio::FileType::Directory {
                                win.add_folder_to_queue(&f);
                            } else {
                                warn!("Unsupported file type {:?} for file '{}'", info.file_type(), f.uri());
                            }
                        }
                    }

                    return true;
                }

                false
            }),
        );

        self.imp().drag_overlay.set_drop_target(&drop_target);
    }

    fn update_playlist_time(&self) {
        let title = format!("<b>{}</b>", &i18n("Playlist"));

        let queue = self.imp().player.queue();
        let n_songs = queue.n_songs();
        if let Some(current) = queue.current_song_index() {
            let mut remaining_time = 0;
            for pos in 0..n_songs {
                let song = queue.song_at(pos).unwrap();
                if pos >= current {
                    remaining_time += song.duration();
                }
            }

            let remaining_min = (remaining_time / 60) as u32;
            let remaining_str = &ni18n_f(
                // Translators: The first '{}' is the word "Playlist";
                // the second '{}' is the number of minutes remaining
                // in the playlist
                "{} ({} minute remaining)",
                "{} ({} minutes remaining)",
                remaining_min,
                &[&title, &remaining_min.to_string()],
            );
            self.imp().queue_length_label.set_label(remaining_str);
        } else {
            self.imp().queue_length_label.set_label(&title);
        }
    }

    fn scroll_playlist_to_song(&self) {
        let queue_view = self.imp().queue_view.get();
        if let Some(current_idx) = self.imp().player.queue().current_song_index() {
            debug!("Scrolling playlist to {}", current_idx);
            queue_view
                .upcast_ref::<gtk::Widget>()
                .activate_action("list.scroll-to-item", Some(&current_idx.to_variant()))
                .expect("Failed to activate action");
        }
    }

    fn setup_provider(&self) {
        let imp = self.imp();
        if let Some(display) = gdk::Display::default() {
            gtk::StyleContext::add_provider_for_display(&display, &imp.provider, 400);
        }
    }

    fn update_style(&self, song: &Song) {
        let imp = self.imp();

        if let Some(bg_colors) = song.cover_palette() {
            // The color chosen depends on the linear gradient we use in the
            // style, so remember to change this when changing the main-window
            // CSS class
            let fg_color = if utils::is_color_dark(&bg_colors[1]) {
                gdk::RGBA::parse("#ffffff").unwrap()
            } else {
                gdk::RGBA::parse("rgba(0, 0, 0, 0.8)").unwrap()
            };

            let mut css = String::new();

            let n_colors = bg_colors.len();
            for (i, color) in bg_colors.iter().enumerate().take(n_colors) {
                css.push_str(&format!("@define-color background_color_{} {};", i, color));
            }

            css.push_str(&format!("@define-color foreground_color {};", fg_color));

            imp.provider.load_from_data(css.as_bytes());
            if !self.has_css_class("main-window") {
                self.add_css_class("main-window");
            }
        } else {
            self.remove_css_class("main-window");
        }
    }

    fn update_waveform(&self, song: Option<&Song>) {
        let imp = self.imp();

        if let Some(song) = song {
            imp.waveform.set_uri(Some(song.uri()));
            imp.waveform.generate_peaks();
        } else {
            imp.waveform.set_uri(None);
            imp.playback_control.waveform_view().set_peaks(None);
        }
    }

    pub fn open_file(&self, file: &gio::File) {
        self.add_file_to_queue(file);
        let queue = self.imp().player.queue();
        if queue.n_songs() == 1 {
            self.imp().player.play();
        }
    }

    pub fn remove_song(&self, song: &Song) {
        let queue = self.imp().player.queue();
        queue.remove_song(song);
    }
}
