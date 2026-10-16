//! Pictures that users upload: avatars and the covers of books, shelves and
//! audiobooks. Nothing is stored or served on the word of the browser's
//! Content-Type: the bytes must decode as a bitmap, and the type stored is
//! the one the decoder found. An SVG never decodes, so script never gets
//! onto the app's origin that way; the response carries nosniff, so a
//! browser does not second-guess the type either.

use crate::AppState;
use axum::extract::multipart::{Multipart, MultipartError};
use axum::http::{header, HeaderName, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use image::ImageFormat;

/// A cover (book, shelf, audiobook) and an avatar, as uploaded.
pub(crate) const MAX_COVER_BYTES: usize = 10 * 1024 * 1024;
pub(crate) const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;

/// An avatar is scaled to fit this many pixels each way.
const AVATAR_PIXELS: u32 = 512;

/// Decoding stops at a picture that would need more memory than this, or
/// is wider or taller than this: a small file can describe an enormous
/// image.
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
const MAX_PIXELS_EACH_WAY: u32 = 12_000;

/// The types a browser is told. Anything else that happens to be stored (an
/// EPUB may declare its cover as it likes) goes out as plain bytes.
const BITMAP_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/gif"];

/// Content-Type and X-Content-Type-Options for a stored picture.
pub(crate) fn headers(mime: &str) -> [(HeaderName, String); 2] {
    let plain = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let served = if BITMAP_TYPES.contains(&plain.as_str()) { plain } else { "application/octet-stream".to_string() };
    [(header::CONTENT_TYPE, served), (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string())]
}

fn bitmap_type(format: ImageFormat) -> Option<&'static str> {
    Some(match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        ImageFormat::WebP => "image/webp",
        ImageFormat::Gif => "image/gif",
        _ => return None,
    })
}

/// The whole picture, decoded, and the format it was in. The format is
/// read from the bytes, never from a name or a header.
fn decode(bytes: &[u8]) -> anyhow::Result<(image::DynamicImage, ImageFormat)> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let format = reader.format().ok_or_else(|| anyhow::anyhow!("not a known image format"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PIXELS_EACH_WAY);
    limits.max_image_height = Some(MAX_PIXELS_EACH_WAY);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    Ok((reader.decode()?, format))
}

fn refused(msg: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response()
}

/// A multipart body that could not be read: 413 past the route's body
/// limit, 400 for anything else.
pub(crate) fn multipart_error(e: MultipartError) -> Response {
    let status = e.status();
    if status != StatusCode::PAYLOAD_TOO_LARGE {
        tracing::warn!("upload: could not read the multipart body: {e}");
    }
    (status, Json(serde_json::json!({ "error": e.body_text() }))).into_response()
}

/// The picture in a multipart upload: the first field sent as an image,
/// read up to `max` bytes and not a byte further.
async fn picture_field(multipart: &mut Multipart, max: usize) -> Result<Vec<u8>, Response> {
    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        if !field.content_type().unwrap_or("").starts_with("image/") {
            continue;
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            if bytes.len() + chunk.len() > max {
                return Err(refused(&format!("image too large (max {} MB)", max / (1024 * 1024))));
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
    }
    Err(refused("no image file in upload"))
}

/// A cover from an upload: the bytes as they came, since they also go into
/// the EPUB, with the type the decoder found. Decoding the whole picture is
/// the check; it runs off the request threads, two at a time like the
/// thumbnails.
pub(crate) async fn cover(state: &AppState, multipart: &mut Multipart) -> Result<(Vec<u8>, String), Response> {
    let bytes = picture_field(multipart, MAX_COVER_BYTES).await?;
    let _gate = state.thumb_gate.acquire().await.ok();
    tokio::task::spawn_blocking(move || {
        let (_, format) = decode(&bytes).ok()?;
        Some((bytes, bitmap_type(format)?.to_string()))
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| refused("not an image"))
}

/// An avatar from an upload, made anew: at most AVATAR_PIXELS each way,
/// nothing but the pixels (no metadata), PNG when it has transparency and
/// JPEG otherwise.
pub(crate) async fn avatar(state: &AppState, multipart: &mut Multipart) -> Result<(Vec<u8>, String), Response> {
    let bytes = picture_field(multipart, MAX_AVATAR_BYTES).await?;
    let _gate = state.thumb_gate.acquire().await.ok();
    tokio::task::spawn_blocking(move || {
        let (img, _) = decode(&bytes).ok()?;
        let img = if img.width() > AVATAR_PIXELS || img.height() > AVATAR_PIXELS {
            img.thumbnail(AVATAR_PIXELS, AVATAR_PIXELS)
        } else {
            img
        };
        let mut out = Vec::new();
        if img.color().has_alpha() {
            img.to_rgba8().write_with_encoder(image::codecs::png::PngEncoder::new(&mut out)).ok()?;
            Some((out, "image/png".to_string()))
        } else {
            img.to_rgb8().write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)).ok()?;
            Some((out, "image/jpeg".to_string()))
        }
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| refused("not an image"))
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_book, add_user, send, test_app};
    use axum::body::Body;
    use axum::http::{header, HeaderMap, Method, Request, StatusCode};
    use axum::Router;
    use serde_json::{json, Value};
    use tower::ServiceExt;

    #[test]
    fn only_bitmap_types_are_served_as_such() {
        let kind = |mime: &str| super::headers(mime)[0].1.clone();
        assert_eq!(kind("image/png"), "image/png");
        assert_eq!(kind("IMAGE/JPEG; charset=utf-8"), "image/jpeg");
        assert_eq!(kind("image/svg+xml"), "application/octet-stream");
        assert_eq!(kind("text/html"), "application/octet-stream");
        assert_eq!(super::headers("image/gif")[1], (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()));
    }

    /// A picture of one colour, as the bytes of a PNG file.
    fn png(width: u32, height: u32, alpha: bool) -> Vec<u8> {
        let mut out = Vec::new();
        if alpha {
            image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 128]))
                .write_with_encoder(image::codecs::png::PngEncoder::new(&mut out))
                .unwrap();
        } else {
            image::RgbImage::from_pixel(width, height, image::Rgb([10, 20, 30]))
                .write_with_encoder(image::codecs::png::PngEncoder::new(&mut out))
                .unwrap();
        }
        out
    }

    /// One file as multipart/form-data, with the type the browser would send.
    async fn post_picture(app: &Router, uri: &str, cookie: &str, mime: &str, bytes: &[u8]) -> (StatusCode, Value) {
        let boundary = "legejo-test-boundary";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"picture\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn get(app: &Router, uri: &str, cookie: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
        let req = Request::builder().uri(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let (status, headers) = (res.status(), res.headers().clone());
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, headers, bytes.to_vec())
    }

    fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
        headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
    }

    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(document.cookie)</script></svg>"#;

    #[tokio::test]
    async fn an_avatar_is_decoded_scaled_and_served_as_a_bitmap() {
        let (app, db, _) = test_app().await;
        let (ada, cookie) = add_user(&db, "ada").await;
        let uri = format!("/api/users/{ada}/avatar");

        // Not a bitmap: refused, whatever the browser calls it.
        let (status, v) = post_picture(&app, "/api/account/avatar", &cookie, "image/svg+xml", SVG).await;
        assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("not an image")));
        let (status, _) = post_picture(&app, "/api/account/avatar", &cookie, "image/png", SVG).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, v) = post_picture(&app, "/api/account/avatar", &cookie, "text/plain", &png(4, 4, false)).await;
        assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("no image file in upload")));
        assert_eq!(get(&app, &uri, &cookie).await.0, StatusCode::NOT_FOUND);

        // A large opaque picture comes back as a small JPEG, with nosniff.
        let (status, _) = post_picture(&app, "/api/account/avatar", &cookie, "image/png", &png(1200, 600, false)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, headers, body) = get(&app, &uri, &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(header(&headers, "content-type"), "image/jpeg");
        assert_eq!(header(&headers, "x-content-type-options"), "nosniff");
        let img = image::load_from_memory(&body).unwrap();
        assert_eq!((img.width(), img.height()), (512, 256));
        // Transparency keeps it a PNG, at its own size.
        let (status, _) = post_picture(&app, "/api/account/avatar", &cookie, "image/png", &png(10, 10, true)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, headers, body) = get(&app, &uri, &cookie).await;
        assert_eq!(header(&headers, "content-type"), "image/png");
        assert_eq!(image::load_from_memory(&body).unwrap().width(), 10);

        // A type stored before this check is served as plain bytes.
        sqlx::query("UPDATE users SET avatar_mime = 'image/svg+xml' WHERE id = ?").bind(ada).execute(&db).await.unwrap();
        let (status, headers, _) = get(&app, &uri, &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(header(&headers, "content-type"), "application/octet-stream");
        assert_eq!(header(&headers, "x-content-type-options"), "nosniff");

        // The file is capped before it is read whole.
        let (status, v) = post_picture(&app, "/api/account/avatar", &cookie, "image/png", &vec![0u8; super::MAX_AVATAR_BYTES + 1]).await;
        assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("image too large (max 5 MB)")));
    }

    #[tokio::test]
    async fn covers_take_the_type_the_decoder_finds() {
        let (app, db, _) = test_app().await;
        let (ada, cookie) = add_user(&db, "ada").await;
        let (book, _) = add_book(&db, ada, "Röda rummet").await;
        let (_, shelf) = send(&app, Method::POST, "/api/shelves", Some(&cookie), Some(json!({ "name": "Hyllan" }))).await;
        sqlx::query("INSERT INTO settings (key, value) VALUES ('audiobooks_enabled', 'true')").execute(&db).await.unwrap();
        let (_, audiobook) = send(&app, Method::POST, "/api/audiobooks", Some(&cookie), Some(json!({ "title": "Hemsöborna" }))).await;
        let book_cover = format!("/api/books/{book}/cover");
        let shelf_cover = format!("/api/shelves/{}/cover", shelf["id"]);
        let audiobook_cover = format!("/api/audiobooks/{}/cover", audiobook["id"]);

        // An SVG is refused for a book, a shelf and an audiobook alike.
        for uri in [&book_cover, &shelf_cover, &audiobook_cover] {
            let (status, v) = post_picture(&app, uri, &cookie, "image/svg+xml", SVG).await;
            assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("not an image")), "{uri}");
            assert_eq!(get(&app, uri, &cookie).await.0, StatusCode::NOT_FOUND, "{uri}");
        }

        // PNG bytes the browser calls SVG are stored and served as PNG.
        for uri in [&book_cover, &shelf_cover, &audiobook_cover] {
            let (status, _) = post_picture(&app, uri, &cookie, "image/svg+xml", &png(4, 4, false)).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            let (status, headers, body) = get(&app, uri, &cookie).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert_eq!(header(&headers, "content-type"), "image/png", "{uri}");
            assert_eq!(header(&headers, "x-content-type-options"), "nosniff", "{uri}");
            assert_eq!(body, png(4, 4, false), "{uri}");
        }
        let stored: String = sqlx::query_scalar("SELECT cover_mime FROM books WHERE id = ?").bind(book).fetch_one(&db).await.unwrap();
        assert_eq!(stored, "image/png");
        // The thumbnail is a JPEG, said the same way.
        let (_, headers, _) = get(&app, &format!("{book_cover}?size=thumb"), &cookie).await;
        assert_eq!(header(&headers, "content-type"), "image/jpeg");
        assert_eq!(header(&headers, "x-content-type-options"), "nosniff");

        // A cover declared by an EPUB as something else goes out as plain bytes.
        sqlx::query("UPDATE books SET cover_mime = 'image/svg+xml' WHERE id = ?").bind(book).execute(&db).await.unwrap();
        let (status, headers, _) = get(&app, &book_cover, &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(header(&headers, "content-type"), "application/octet-stream");
    }
}
