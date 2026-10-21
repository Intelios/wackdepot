//! Image diffs: decoding the raw bytes [`wackdepot_core::blob`] returns for
//! binary files into `RenderImage`s the diff card can draw side by side.
//!
//! Decoding happens here — on the background diff-load thread — not through
//! gpui's asset cache, so `Arc<RenderImage>` lifetimes track the `DiffDoc`
//! that owns them. Decoding mirrors gpui's own `Image::to_image_data` (same
//! `image` crate feature set, same RGB→BGR swap, GIF and animated WebP keep
//! their frames).

use std::io::Cursor;
use std::sync::Arc;

use gpui::RenderImage;
use image::{
    AnimationDecoder, DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader, Rgba,
    codecs::{gif::GifDecoder, webp::WebPDecoder},
};
use smallvec::SmallVec;
use wackdepot_core::blob::ByteVersions;

/// Past this many pixels an image is too big to show — it's a data file
/// wearing an image extension, and decoding it would eat hundreds of MB.
const MAX_PIXELS: u64 = 128 * 1024 * 1024;

/// An animation with more frames than this gets its first frame only — a
/// diff preview doesn't need to play a whole filmstrip.
const MAX_FRAMES: usize = 64;

/// One side of an image diff, decoded and measured.
pub struct SideImage {
    /// The decoded image (animated formats keep their frames).
    pub image: Arc<RenderImage>,
    /// Encoded size in bytes, for the caption.
    pub bytes: usize,
    /// Pixel dimensions, `(w, h)`, orientation applied.
    pub dims: (u32, u32),
}

impl std::fmt::Debug for SideImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SideImage")
            .field("bytes", &self.bytes)
            .field("dims", &self.dims)
            .finish_non_exhaustive()
    }
}

/// The two sides of an image diff. `None` where the side doesn't exist (an
/// added or deleted file) or its bytes didn't decode to an image — then the
/// card falls back to the binary stub.
#[derive(Debug, Default)]
pub struct ImageVersions {
    pub old: Option<SideImage>,
    pub new: Option<SideImage>,
}

impl ImageVersions {
    /// Whether at least one side decoded to an image.
    pub fn any(&self) -> bool {
        self.old.is_some() || self.new.is_some()
    }

    /// Both sides in order — for eviction of the `RenderImage`s a doc held.
    pub fn iter(&self) -> impl Iterator<Item = &SideImage> {
        [self.old.as_ref(), self.new.as_ref()].into_iter().flatten()
    }
}

/// Decode the sides of `v` that exist. Decoding failures come back `None`;
/// callers fall back to the generic binary state.
pub fn pair(v: &ByteVersions) -> ImageVersions {
    ImageVersions {
        old: v.old.as_deref().and_then(decode),
        new: v.new.as_deref().and_then(decode),
    }
}

/// Decode one side's bytes — `None` when the content isn't a decodable
/// image (or tops `MAX_PIXELS`), so a `.zip` never reaches the renderer.
fn decode(bytes: &[u8]) -> Option<SideImage> {
    let format = image::guess_format(bytes).ok()?;
    let frames = match format {
        ImageFormat::Gif => {
            let decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
            if !within_limits(decoder.dimensions()) {
                return None;
            }
            collect_frames(decoder)
        }
        ImageFormat::WebP => {
            let mut decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
            if !within_limits(decoder.dimensions()) {
                return None;
            }
            if decoder.has_animation() {
                let _ = decoder.set_background_color(Rgba([0, 0, 0, 0]));
                collect_frames(decoder)
            } else {
                vec![static_frame(decoder)?]
            }
        }
        _ => vec![static_frame(
            ImageReader::with_format(Cursor::new(bytes), format)
                .into_decoder()
                .ok()?,
        )?],
    };
    if frames.is_empty() {
        return None;
    }
    let dims = frames[0].buffer().dimensions();
    Some(SideImage {
        image: Arc::new(RenderImage::new(SmallVec::from_vec(frames))),
        bytes: bytes.len(),
        dims,
    })
}

/// Every decodable frame (capped at `MAX_FRAMES`), RGBA→BGRA-swapped.
fn collect_frames<'a>(decoder: impl AnimationDecoder<'a>) -> Vec<Frame> {
    let mut frames = Vec::new();
    for frame in decoder.into_frames().take(MAX_FRAMES) {
        match frame {
            Ok(mut frame) => {
                swap(frame.buffer_mut().as_mut());
                frames.push(frame);
            }
            Err(_) => break,
        }
    }
    frames
}

/// The single (first) frame of a static image, orientation applied.
fn static_frame(mut decoder: impl ImageDecoder) -> Option<Frame> {
    if !within_limits(decoder.dimensions()) {
        return None;
    }
    let orientation = decoder.orientation().ok()?;
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let mut data = image.into_rgba8();
    swap(data.as_mut());
    Some(Frame::new(data))
}

fn within_limits((w, h): (u32, u32)) -> bool {
    w > 0 && h > 0 && w as u64 * h as u64 <= MAX_PIXELS
}

fn swap(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
}

/// `1234567` → `"1.2 MB"` — the byte-size half of an image pane's caption.
pub fn byte_size(n: usize) -> String {
    const KB: f64 = 1024.0;
    let n = n as f64;
    if n < KB {
        format!("{} B", n as usize)
    } else if n < KB * KB {
        format!("{:.1} KB", n / KB)
    } else if n < KB * KB * KB {
        format!("{:.1} MB", n / (KB * KB))
    } else {
        format!("{:.1} GB", n / (KB * KB * KB))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageBuffer;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        let img = ImageBuffer::from_pixel(w, h, Rgba([200u8, 30, 30, 255]));
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_a_png() {
        let bytes = png(24, 12);
        let side = decode(&bytes).unwrap();
        assert_eq!(side.dims, (24, 12));
        assert_eq!(side.bytes, bytes.len());
        assert_eq!(side.image.frame_count(), 1);
    }

    #[test]
    fn rejects_non_images_and_giants() {
        assert!(decode(b"not an image, just bytes").is_none());
        assert!(decode(&[]).is_none());

        // A real PNG header claiming 1M×1M pixels — refused before decode.
        let mut fake = Vec::from(&b"\x89PNG\r\n\x1a\n"[..]);
        fake.extend_from_slice(&13u32.to_be_bytes()); // IHDR len
        fake.extend_from_slice(b"IHDR");
        fake.extend_from_slice(&1_000_000u32.to_be_bytes());
        fake.extend_from_slice(&1_000_000u32.to_be_bytes());
        fake.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert!(decode(&fake).is_none());
    }

    #[test]
    fn pair_keeps_missing_sides() {
        let v = ByteVersions {
            old: None,
            new: Some(png(4, 4)),
        };
        let p = pair(&v);
        assert!(p.any());
        assert!(p.old.is_none());
        assert_eq!(p.new.as_ref().unwrap().dims, (4, 4));

        let none = pair(&ByteVersions::default());
        assert!(!none.any());
    }

    #[test]
    fn byte_size_formats() {
        assert_eq!(byte_size(0), "0 B");
        assert_eq!(byte_size(512), "512 B");
        assert_eq!(byte_size(1024), "1.0 KB");
        assert_eq!(byte_size(1536), "1.5 KB");
        assert_eq!(byte_size(1_234_567), "1.2 MB");
        assert_eq!(byte_size(3_000_000_000), "2.8 GB");
    }
}
