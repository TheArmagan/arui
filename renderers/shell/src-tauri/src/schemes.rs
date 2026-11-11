//! Custom URI schemes. WebView2 exposes them as `http://<scheme>.localhost/...`.
//! * `arui-icon`: PNGs from the shared IconStore, path = url-encoded key.
//! * `arui-mod`:  files from %APPDATA%\ARUI\mods\<id>\..., with CORS so `import()` works.

use std::borrow::Cow;
use std::path::Path;

use arui_win::IconStore;
use tauri::http::{header, Response, StatusCode};

fn base() -> tauri::http::response::Builder {
    Response::builder().header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    base()
        .status(StatusCode::NOT_FOUND)
        .body(Cow::Borrowed(&b""[..]))
        .unwrap()
}

pub fn icon(store: &IconStore, path: &str) -> Response<Cow<'static, [u8]>> {
    let key = percent_encoding::percent_decode_str(path.trim_start_matches('/'))
        .decode_utf8_lossy()
        .to_string();
    match store.get(&key) {
        Some(png) => base()
            .header(header::CONTENT_TYPE, "image/png")
            // Keys change when content changes (tray/art), so short caching is safe.
            .header(header::CACHE_CONTROL, "max-age=300")
            .body(Cow::Owned(png.as_ref().clone()))
            .unwrap(),
        None => not_found(),
    }
}

pub fn mod_file(mods_dir: &Path, path: &str) -> Response<Cow<'static, [u8]>> {
    let Some(file) = crate::mods::resolve_file(mods_dir, path) else { return not_found() };
    match std::fs::read(&file) {
        Ok(bytes) => base()
            .header(header::CONTENT_TYPE, crate::mods::content_type(&file))
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Cow::Owned(bytes))
            .unwrap(),
        Err(_) => not_found(),
    }
}
