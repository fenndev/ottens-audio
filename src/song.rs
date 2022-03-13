// SPDX-FileCopyrightText: 2022  Emmanuele Bassi
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;

use glib::{ParamFlags, ParamSpec, ParamSpecString, ParamSpecUInt, Value};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use lofty::Accessor;
use once_cell::sync::Lazy;

use crate::i18n::i18n;

#[derive(Debug, Clone)]
pub struct SongData {
    artist: Option<String>,
    title: Option<String>,
    album: Option<String>,
    cover_art: Option<glib::Bytes>,
    duration: u64,
    file: gio::File,
}

impl SongData {
    pub fn artist(&self) -> Option<&str> {
        self.artist.as_ref().map(|s| s.as_str())
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_ref().map(|s| s.as_str())
    }

    pub fn album(&self) -> Option<&str> {
        self.album.as_ref().map(|s| s.as_str())
    }

    pub fn duration(&self) -> u64 {
        self.duration
    }

    pub fn cover_art(&self) -> Option<&glib::Bytes> {
        self.cover_art.as_ref()
    }

    pub fn from_uri(uri: &str) -> Self {
        let file = gio::File::for_uri(uri);
        let path = file.path().expect("Unable to find file");

        let tagged_file = lofty::read_from_path(&path, true).expect("Unable to open file");

        let mut artist = None;
        let mut title = None;
        let mut album = None;
        let mut cover_art = None;
        if let Some(tag) = tagged_file.primary_tag() {
            artist = tag.artist().map(|s| s.to_string());
            title = tag.title().map(|s| s.to_string());
            album = tag.album().map(|s| s.to_string());
            for picture in tag.pictures() {
                match picture.mime_type() {
                    lofty::MimeType::Png => {
                        cover_art = Some(glib::Bytes::from(picture.data()));
                    }
                    lofty::MimeType::Jpeg => {
                        cover_art = Some(glib::Bytes::from(picture.data()));
                    }
                    lofty::MimeType::Tiff => {
                        cover_art = Some(glib::Bytes::from(picture.data()));
                    }
                    _ => cover_art = None,
                }
            }
        } else {
            warn!("Unable to load tags for {}", uri);
        };

        let duration = tagged_file.properties().duration().as_secs();

        SongData {
            artist,
            title,
            album,
            cover_art,
            duration,
            file,
        }
    }

    pub fn uri(&self) -> String {
        self.file.uri().to_string()
    }
}

impl Default for SongData {
    fn default() -> Self {
        SongData {
            artist: None,
            title: None,
            album: None,
            cover_art: None,
            duration: 0,
            file: gio::File::for_path("/does-not-exist"),
        }
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Song {
        pub data: RefCell<SongData>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Song {
        const NAME: &'static str = "AmberolSong";
        type Type = super::Song;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for Song {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::new("uri", "", "", None, ParamFlags::READWRITE),
                    ParamSpecString::new("artist", "", "", None, ParamFlags::READABLE),
                    ParamSpecString::new("title", "", "", None, ParamFlags::READABLE),
                    ParamSpecString::new("album", "", "", None, ParamFlags::READABLE),
                    ParamSpecUInt::new("duration", "", "", 0, u32::MAX, 0, ParamFlags::READABLE),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _obj: &Self::Type, _id: usize, value: &Value, pspec: &ParamSpec) {
            match pspec.name() {
                "uri" => {
                    let p = value.get::<&str>().expect("The value needs to be a string");
                    self.data.replace(SongData::from_uri(p));
                    _obj.notify("artist");
                    _obj.notify("title");
                    _obj.notify("album");
                    _obj.notify("duration");
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &ParamSpec) -> Value {
            match pspec.name() {
                "artist" => self.data.borrow().artist().to_value(),
                "title" => self.data.borrow().title().to_value(),
                "album" => self.data.borrow().album().to_value(),
                "duration" => self.data.borrow().duration().to_value(),
                "uri" => self.data.borrow().uri().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Song(ObjectSubclass<imp::Song>);
}

impl Song {
    pub fn new(uri: &str) -> Self {
        glib::Object::new::<Self>(&[("uri", &uri)]).expect("Failed to create Song object")
    }

    pub fn empty() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create an empty Song object")
    }

    pub fn equals(&self, other: &Self) -> bool {
        self.uri() == other.uri()
    }

    fn imp(&self) -> &imp::Song {
        imp::Song::from_instance(self)
    }

    pub fn uri(&self) -> String {
        self.imp().data.borrow().uri()
    }

    pub fn artist(&self) -> String {
        match self.imp().data.borrow().artist() {
            Some(artist) => return artist.to_string(),
            None => return i18n("Unknown artist").to_string(),
        }
    }

    pub fn title(&self) -> String {
        match self.imp().data.borrow().title() {
            Some(title) => return title.to_string(),
            None => return i18n("Unknown title").to_string(),
        }
    }

    pub fn album(&self) -> String {
        match self.imp().data.borrow().album() {
            Some(album) => return album.to_string(),
            None => return i18n("Unknown album").to_string(),
        }
    }

    pub fn cover_art(&self) -> Option<glib::Bytes> {
        match self.imp().data.borrow().cover_art() {
            Some(buffer) => Some(buffer.clone()),
            None => None,
        }
    }

    pub fn duration(&self) -> u64 {
        self.imp().data.borrow().duration()
    }
}

impl Default for Song {
    fn default() -> Self {
        Self::empty()
    }
}
